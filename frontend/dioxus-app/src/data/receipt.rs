//! `data::receipt` — `/receipt` (T41): one photo in, one staged proposal out.
//!
//! The photo is hostile input. The server fn streams it with a hard byte cap
//! before anything else sees it, then hands it to
//! `phosk_pipeline_receipt::intake_receipt`. Only JPEG and PNG get that far:
//! they are the formats the pipeline strips EXIF from, so any other signature
//! is refused here first. Intake validates the bytes, strips EXIF, stores the
//! photo encrypted, dedups by content hash and STAGES a
//! proposal. Nothing here books: the review screen approves or rejects through
//! the T42 fns in `data::approvals`. Failures map to fixed user-facing texts;
//! a `PhoskError` or adapter string never reaches the client.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::data::approvals::ProposalDto;

/// Largest accepted photo (12 MiB), the pipeline's own cap (a test pins them
/// together). Public so the page can refuse a too-big file before sending it.
pub const MAX_UPLOAD_BYTES: usize = 12 * 1024 * 1024;

/// What intake did with the photo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IntakeStatus {
    /// A new proposal was staged for review.
    Staged,
    /// This exact photo was submitted before; nothing new was staged.
    Duplicate,
}

/// The result of one upload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptIntakeDto {
    /// Staged or duplicate.
    pub status: IntakeStatus,
    /// The approval-queue suggestion id (approve / reject target).
    pub suggestion_id: String,
    /// The pending proposal to review; `None` once it is no longer pending
    /// (a duplicate of an already approved photo).
    pub proposal: Option<ProposalDto>,
    /// Another open proposal targets the same receipt: approve is refused
    /// until all but one are rejected (as on /approvals).
    pub conflicting: bool,
}

/// Upload one receipt photo and stage its proposal for review.
///
/// REAL: composes `phosk_pipeline_receipt::intake_receipt` (never books).
#[server]
pub async fn upload_receipt(
    photo: dioxus::fullstack::FileStream,
) -> Result<ReceiptIntakeDto, ServerFnError> {
    #[cfg(feature = "server-deps")]
    {
        let bytes = read_capped(photo).await?;
        let session = crate::data::build_session()
            .await
            .map_err(|_| ServerFnError::new(RETRY))?;
        upload_receipt_with(
            session.db(),
            session.storage(),
            session.ocr(),
            session.llm(),
            &bytes,
            crate::data::today(),
        )
        .await
    }
    #[cfg(not(feature = "server-deps"))]
    {
        let _ = photo;
        Err(ServerFnError::new("server-only"))
    }
}

/// Read the streamed photo into memory, refusing past `MAX_UPLOAD_BYTES`.
/// The declared size is a hint only; `to_bytes` enforces the cap on the bytes
/// actually read and stops at the limit, so an oversize body is never
/// buffered whole.
#[cfg(feature = "server-deps")]
pub(crate) async fn read_capped(
    photo: dioxus::fullstack::FileStream,
) -> Result<dioxus::fullstack::body::Bytes, ServerFnError> {
    use dioxus::fullstack::body::{to_bytes, Body};
    if photo
        .size()
        .is_some_and(|n| usize::try_from(n).map_or(true, |n| n > MAX_UPLOAD_BYTES))
    {
        return Err(ServerFnError::new(TOO_LARGE));
    }
    to_bytes(Body::from_stream(photo), MAX_UPLOAD_BYTES)
        .await
        .map_err(|_| ServerFnError::new(CUT))
}

#[cfg(feature = "server-deps")]
const TOO_LARGE: &str = "This photo is larger than 12 MB. Take a smaller photo and try again.";
#[cfg(feature = "server-deps")]
const CUT: &str = "The upload failed or went over 12 MB. Try again with a smaller photo.";
#[cfg(feature = "server-deps")]
const NO_PHOTO: &str = "No photo was received. Pick a photo and try again.";
#[cfg(feature = "server-deps")]
const NOT_JPEG_PNG: &str = "This file is not a JPEG or PNG photo. Nothing was staged. \
                            Upload a JPEG or PNG photo of one receipt.";
#[cfg(feature = "server-deps")]
const UNREADABLE: &str = "Could not read a receipt from this photo. Nothing was staged. \
                          Try a clearer photo or try again later.";
#[cfg(feature = "server-deps")]
const RETRY: &str = "Could not process the photo right now. Please try again.";
#[cfg(feature = "server-deps")]
const NO_REVIEW: &str = "The receipt was staged, but its review could not be loaded. \
                         Open Approvals to review it.";

/// Cap- and signature-check `bytes`, run intake with the given ports, then load the staged
/// proposal for review (the same mapping the approval queue shows).
#[cfg(feature = "server-deps")]
pub(crate) async fn upload_receipt_with(
    db: &dyn phosk_adapter_db::DatabaseAdapter,
    storage: &dyn phosk_adapter_storage::PhotoStorage,
    ocr: &dyn phosk_adapter_ocr::OcrAdapter,
    llm: &dyn phosk_adapter_llm::LlmAdapter,
    bytes: &[u8],
    today: chrono::NaiveDate,
) -> Result<ReceiptIntakeDto, ServerFnError> {
    use crate::data::approvals::get_proposal_with;
    use phosk_core::error::PhoskError;
    use phosk_pipeline_receipt::{intake_receipt, IntakePhoto};

    // The size cap runs before intake, so an oversize body never reaches it.
    if bytes.len() > MAX_UPLOAD_BYTES {
        return Err(ServerFnError::new(TOO_LARGE));
    }
    if bytes.is_empty() {
        return Err(ServerFnError::new(NO_PHOTO));
    }
    if !is_jpeg_or_png(bytes) {
        return Err(ServerFnError::new(NOT_JPEG_PNG));
    }
    let photo = IntakePhoto {
        bytes,
        captured_on: today,
    };
    let out = intake_receipt(db, storage, ocr, llm, photo)
        .await
        .map_err(|e| match e {
            // A bad photo, an unusable model reading, or a model outage (the
            // LLM adapter reports those as `Invalid` too): don't blame one.
            PhoskError::Invalid(_) | PhoskError::InvalidDate(_) => ServerFnError::new(UNREADABLE),
            PhoskError::NotFound(_) | PhoskError::Overflow(_) => ServerFnError::new(RETRY),
        })?;
    let suggestion_id = out.suggestion_id.to_string();
    let proposal = match get_proposal_with(db, &suggestion_id).await {
        Ok(p) => Some(p),
        // A duplicate of a photo whose proposal was already decided.
        Err(e) if out.deduplicated && is_gone(&e) => None,
        Err(_) => return Err(ServerFnError::new(NO_REVIEW)),
    };
    let conflicting = match proposal {
        Some(_) => has_rival(db, &suggestion_id)
            .await
            .map_err(|_| ServerFnError::new(NO_REVIEW))?,
        None => false,
    };
    Ok(ReceiptIntakeDto {
        status: if out.deduplicated {
            IntakeStatus::Duplicate
        } else {
            IntakeStatus::Staged
        },
        suggestion_id,
        proposal,
        conflicting,
    })
}

/// The JPEG (`FF D8 FF`) or PNG signature: the formats intake strips EXIF from.
#[cfg(feature = "server-deps")]
fn is_jpeg_or_png(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xFF, 0xD8, 0xFF])
        || bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A])
}

#[cfg(feature = "server-deps")]
fn is_gone(e: &ServerFnError) -> bool {
    matches!(e, ServerFnError::ServerError { message, .. } if message == crate::data::approvals::GONE)
}

/// Another OPEN receipt suggestion targets the same receipt as `id`.
#[cfg(feature = "server-deps")]
async fn has_rival(
    db: &dyn phosk_adapter_db::DatabaseAdapter,
    id: &str,
) -> Result<bool, phosk_core::error::PhoskError> {
    let all = db.ai_suggestions().await?;
    let target = all
        .iter()
        .find(|s| s.id.to_string() == id)
        .and_then(|s| s.target.clone());
    Ok(target.is_some_and(|t| {
        all.iter()
            .filter(|s| s.kind == "receipt" && s.status == "open")
            .filter(|s| s.target.as_deref() == Some(t.as_str()))
            .count()
            > 1
    }))
}
