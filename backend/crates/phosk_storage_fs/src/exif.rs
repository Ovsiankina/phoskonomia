//! Pure-Rust metadata stripping for receipt photos (ADR §0: EXIF stripped on
//! import). `exiftool` is intentionally absent on the target machine, so this is
//! a small, dependency-free segment/chunk rewriter that drops metadata while
//! leaving the decoded pixels bit-for-bit identical.
//!
//! Two container formats are recognised, matching what phone cameras emit:
//!
//! * **JPEG** — every `APPn` marker segment (`0xFFE0`..=`0xFFEF`, which covers
//!   `APP1`/EXIF, `APP1`/XMP, `APP0`/JFIF, `APP2`/ICC, `APP13`/IPTC, …) and the
//!   `COM` comment segment are removed. Entropy-coded scan data and all
//!   structural markers are copied verbatim, so the image still decodes.
//! * **PNG** — every *ancillary* chunk that can carry metadata (`tEXt`, `zTXt`,
//!   `iTXt`, `eXIf`, `tIME`) is removed; all critical chunks (`IHDR`, `PLTE`,
//!   `IDAT`, `IEND`) and other ancillary chunks affecting rendering (e.g.
//!   `gAMA`, `tRNS`) are preserved.
//!
//! Any input that is not a recognised JPEG/PNG is returned **unchanged** — the
//! storage adapter still encrypts it, but this module makes no claim about
//! formats it does not understand (callers validate MIME/libmagic above the
//! port). The strip is best-effort and infallible: a truncated/garbled stream
//! degrades to "return the original bytes" rather than erroring, because the
//! encryption layer is the security boundary, not this cosmetic cleanup.

/// First bytes of a JPEG stream: SOI marker `FF D8`.
const JPEG_SOI: [u8; 2] = [0xFF, 0xD8];
/// 8-byte PNG signature.
const PNG_SIG: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n'];

/// Strip metadata from `bytes`, returning a new owned buffer.
///
/// Detects the container by magic bytes and dispatches; unknown formats and
/// malformed streams round-trip unchanged (see module docs). Pixels are never
/// altered.
pub fn strip_metadata(bytes: &[u8]) -> Vec<u8> {
    if bytes.starts_with(&JPEG_SOI) {
        strip_jpeg(bytes).unwrap_or_else(|| bytes.to_vec())
    } else if bytes.starts_with(&PNG_SIG) {
        strip_png(bytes).unwrap_or_else(|| bytes.to_vec())
    } else {
        bytes.to_vec()
    }
}

/// Rewrite a JPEG, dropping `APPn` and `COM` marker segments.
///
/// Returns `None` (→ caller keeps the original) if the structure is not a
/// well-formed marker stream up to the start-of-scan, so we never emit a
/// corrupted image.
fn strip_jpeg(bytes: &[u8]) -> Option<Vec<u8>> {
    // Must start with SOI.
    if !bytes.starts_with(&JPEG_SOI) {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len());
    out.extend_from_slice(&JPEG_SOI);

    let mut i = 2usize;
    loop {
        // Need at least a 2-byte marker.
        if i + 1 >= bytes.len() {
            return None;
        }
        // Markers are 0xFF followed by a non-0x00, non-0xFF marker code.
        if bytes[i] != 0xFF {
            return None;
        }
        // Skip any fill 0xFF bytes between markers.
        let mut marker_idx = i + 1;
        while marker_idx < bytes.len() && bytes[marker_idx] == 0xFF {
            marker_idx += 1;
        }
        if marker_idx >= bytes.len() {
            return None;
        }
        let marker = bytes[marker_idx];

        // SOS (0xDA): the rest is entropy-coded scan data (+ trailing markers);
        // copy everything from here to the end verbatim and finish.
        if marker == 0xDA {
            out.extend_from_slice(&bytes[i..]);
            return Some(out);
        }

        // Markers without a length payload (standalone): RSTn / TEM. Copy and
        // continue. (SOI/EOI shouldn't appear here mid-stream.)
        if (0xD0..=0xD7).contains(&marker) || marker == 0x01 {
            out.extend_from_slice(&[0xFF, marker]);
            i = marker_idx + 1;
            continue;
        }

        // EOI before any scan — unusual but copy and stop.
        if marker == 0xD9 {
            out.extend_from_slice(&[0xFF, marker]);
            return Some(out);
        }

        // All other markers carry a 2-byte big-endian length (including the two
        // length bytes themselves) followed by that many payload bytes.
        let len_hi = *bytes.get(marker_idx + 1)?;
        let len_lo = *bytes.get(marker_idx + 2)?;
        let seg_len = u16::from_be_bytes([len_hi, len_lo]) as usize;
        if seg_len < 2 {
            return None;
        }
        // Segment spans: [marker_idx+1 .. marker_idx+1+seg_len) (length + body).
        let seg_end = marker_idx + 1 + seg_len;
        if seg_end > bytes.len() {
            return None;
        }

        let is_appn = (0xE0..=0xEF).contains(&marker);
        let is_com = marker == 0xFE;
        if is_appn || is_com {
            // Drop this metadata segment entirely.
        } else {
            // Preserve the marker + its full payload verbatim.
            out.extend_from_slice(&[0xFF, marker]);
            out.extend_from_slice(&bytes[marker_idx + 1..seg_end]);
        }
        i = seg_end;
    }
}

/// Chunk types that may carry metadata and are safe to drop without changing the
/// rendered image.
const fn is_png_metadata_chunk(kind: [u8; 4]) -> bool {
    matches!(&kind, b"tEXt" | b"zTXt" | b"iTXt" | b"eXIf" | b"tIME")
}

/// Rewrite a PNG, dropping ancillary metadata chunks.
///
/// Returns `None` (→ keep original) on any structural malformation so a broken
/// stream is never half-rewritten.
fn strip_png(bytes: &[u8]) -> Option<Vec<u8>> {
    if !bytes.starts_with(&PNG_SIG) {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len());
    out.extend_from_slice(&PNG_SIG);

    let mut i = PNG_SIG.len();
    loop {
        // Each chunk: 4-byte big-endian length, 4-byte type, `length` data,
        // 4-byte CRC.
        if i == bytes.len() {
            // Clean end (IEND should have been the last copied chunk).
            return Some(out);
        }
        if i + 8 > bytes.len() {
            return None;
        }
        let len = u32::from_be_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]) as usize;
        let kind: [u8; 4] = [bytes[i + 4], bytes[i + 5], bytes[i + 6], bytes[i + 7]];
        let chunk_end = i + 8 + len + 4; // header + data + CRC
        if chunk_end > bytes.len() {
            return None;
        }

        if is_png_metadata_chunk(kind) {
            // Drop the whole chunk (header + data + CRC).
        } else {
            out.extend_from_slice(&bytes[i..chunk_end]);
        }

        let is_iend = &kind == b"IEND";
        i = chunk_end;
        if is_iend {
            return Some(out);
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    /// Build a minimal JPEG: SOI, APP0/JFIF, APP1/EXIF, a DQT (preserved),
    /// SOS + fake scan data, EOI.
    fn jpeg_with_exif() -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&[0xFF, 0xD8]); // SOI
        // APP0 JFIF, length 6 (incl length bytes): payload "JFIF\0..."
        v.extend_from_slice(&[0xFF, 0xE0, 0x00, 0x06, b'J', b'F', b'I', b'F']);
        // APP1 EXIF, length 0x000B (11) = 2 length bytes + 9 payload bytes.
        v.extend_from_slice(&[0xFF, 0xE1, 0x00, 0x0B]);
        v.extend_from_slice(b"Exif\0SEC!"); // 9 bytes
        // DQT marker (0xDB), length 4, two payload bytes — must be preserved.
        v.extend_from_slice(&[0xFF, 0xDB, 0x00, 0x04, 0xAA, 0xBB]);
        // SOS (0xDA), length 4, two payload bytes, then scan data + EOI.
        v.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x04, 0x01, 0x02]);
        v.extend_from_slice(&[0x12, 0x34, 0x56]); // entropy-coded scan
        v.extend_from_slice(&[0xFF, 0xD9]); // EOI
        v
    }

    #[test]
    fn jpeg_drops_app_segments_keeps_dqt_and_scan() {
        let input = jpeg_with_exif();
        let out = strip_metadata(&input);
        // EXIF secret string gone.
        assert!(
            !out.windows(4).any(|w| w == b"Exif"),
            "EXIF marker payload must be removed"
        );
        assert!(!out.windows(4).any(|w| w == b"SEC!"));
        assert!(!out.windows(4).any(|w| w == b"JFIF"));
        // Structural markers preserved.
        assert!(out.starts_with(&[0xFF, 0xD8]));
        // DQT payload preserved.
        assert!(out.windows(2).any(|w| w == [0xAA, 0xBB]));
        // Scan data preserved.
        assert!(out.windows(3).any(|w| w == [0x12, 0x34, 0x56]));
        // EOI preserved.
        assert!(out.ends_with(&[0xFF, 0xD9]));
        // It actually shrank.
        assert!(out.len() < input.len());
    }

    /// Minimal PNG: signature, IHDR, tEXt (metadata), IDAT, eXIf, IEND.
    fn png_with_text() -> Vec<u8> {
        fn chunk(kind: [u8; 4], data: &[u8]) -> Vec<u8> {
            let mut c = Vec::new();
            let len = u32::try_from(data.len()).expect("chunk len fits u32");
            c.extend_from_slice(&len.to_be_bytes());
            c.extend_from_slice(&kind);
            c.extend_from_slice(data);
            c.extend_from_slice(&[0, 0, 0, 0]); // fake CRC (we don't verify it)
            c
        }
        let mut v = Vec::new();
        v.extend_from_slice(&PNG_SIG);
        v.extend(chunk(*b"IHDR", &[0; 13]));
        v.extend(chunk(*b"tEXt", b"Comment\0secret-gps"));
        v.extend(chunk(*b"IDAT", &[0xDE, 0xAD, 0xBE, 0xEF]));
        v.extend(chunk(*b"eXIf", b"hidden-exif"));
        v.extend(chunk(*b"IEND", &[]));
        v
    }

    #[test]
    fn png_drops_text_and_exif_keeps_ihdr_idat() {
        let input = png_with_text();
        let out = strip_metadata(&input);
        assert!(out.starts_with(&PNG_SIG));
        assert!(out.windows(4).any(|w| w == b"IHDR"));
        assert!(out.windows(4).any(|w| w == b"IDAT"));
        assert!(out.windows(4).any(|w| w == [0xDE, 0xAD, 0xBE, 0xEF]));
        assert!(out.windows(4).any(|w| w == b"IEND"));
        // Metadata gone.
        assert!(!out.windows(4).any(|w| w == b"tEXt"));
        assert!(!out.windows(4).any(|w| w == b"eXIf"));
        assert!(!out.windows(6).any(|w| w == b"secret"));
        assert!(!out.windows(6).any(|w| w == b"hidden"));
        assert!(out.len() < input.len());
    }

    #[test]
    fn unknown_format_round_trips_unchanged() {
        let input = b"not an image at all".to_vec();
        assert_eq!(strip_metadata(&input), input);
    }

    #[test]
    fn truncated_jpeg_round_trips_unchanged() {
        // SOI then a truncated APP1 length — must not corrupt; returns original.
        let input = vec![0xFF, 0xD8, 0xFF, 0xE1, 0x00];
        assert_eq!(strip_metadata(&input), input);
    }
}
