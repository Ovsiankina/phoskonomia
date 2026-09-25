//! `data::receipt` — `/receipt` (T41): one photo in, one staged proposal out.
//!
//! The photo is hostile input. The server fn streams it with a hard byte cap
//! before anything else sees it, then hands it to
//! `phosk_pipeline_receipt::intake_receipt`, which validates the magic bytes,
//! strips EXIF, stores it encrypted, dedups by content hash and STAGES a
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
        use dioxus::fullstack::body::{to_bytes, Body};
        // The declared size is a hint only; `to_bytes` enforces the cap on
        // the bytes actually read and stops at the limit.
        if photo
            .size()
            .is_some_and(|n| usize::try_from(n).map_or(true, |n| n > MAX_UPLOAD_BYTES))
        {
            return Err(ServerFnError::new(TOO_LARGE));
        }
        let bytes = to_bytes(Body::from_stream(photo), MAX_UPLOAD_BYTES)
            .await
            .map_err(|_| ServerFnError::new(CUT))?;
        let session = crate::data::build_session().await?;
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

#[cfg(feature = "server-deps")]
const TOO_LARGE: &str = "This photo is larger than 12 MB. Take a smaller photo and try again.";
#[cfg(feature = "server-deps")]
const CUT: &str = "The upload failed or went over 12 MB. Try again with a smaller photo.";
#[cfg(feature = "server-deps")]
const NO_PHOTO: &str = "No photo was received. Pick a photo and try again.";
#[cfg(feature = "server-deps")]
const REFUSED: &str = "This file could not be read as a receipt photo. Nothing was staged. \
                       Upload a clear JPEG or PNG photo of one receipt.";
#[cfg(feature = "server-deps")]
const RETRY: &str = "Could not process the photo right now. Please try again.";
#[cfg(feature = "server-deps")]
const NO_REVIEW: &str = "The receipt was staged, but its review could not be loaded. \
                         Open Approvals to review it.";

/// Cap-check `bytes`, run intake with the given ports, then load the staged
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
    use phosk_core::error::PhoskError;
    use phosk_pipeline_receipt::{intake_receipt, IntakePhoto};

    // The size cap runs before intake, so an oversize body never reaches it.
    if bytes.len() > MAX_UPLOAD_BYTES {
        return Err(ServerFnError::new(TOO_LARGE));
    }
    if bytes.is_empty() {
        return Err(ServerFnError::new(NO_PHOTO));
    }
    let photo = IntakePhoto {
        bytes,
        captured_on: today,
    };
    let out = intake_receipt(db, storage, ocr, llm, photo)
        .await
        .map_err(|e| match e {
            // Not an image, or a model reading that can't be approved.
            PhoskError::Invalid(_) | PhoskError::InvalidDate(_) => ServerFnError::new(REFUSED),
            PhoskError::NotFound(_) | PhoskError::Overflow(_) => ServerFnError::new(RETRY),
        })?;
    let suggestion_id = out.suggestion_id.to_string();
    let proposal = crate::data::approvals::list_pending_proposals_with(db)
        .await
        .map_err(|_| ServerFnError::new(NO_REVIEW))?
        .into_iter()
        .flat_map(|g| g.proposals)
        .find(|p| p.suggestion_id == suggestion_id);
    Ok(ReceiptIntakeDto {
        status: if out.deduplicated {
            IntakeStatus::Duplicate
        } else {
            IntakeStatus::Staged
        },
        suggestion_id,
        proposal,
    })
}
