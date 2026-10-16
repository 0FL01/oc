//! Bounded verified attachments for T13 (PROV05).
//!
//! Advertised `text`/`image` kinds only: images are verified by magic bytes
//! (PNG/JPEG/GIF/WEBP), single and total caps enforced before encoding, and
//! audio/video/PDF fail visibly. Encoded request segments stay under the
//! request cap; the model never smuggles credentials through attachments.

use thiserror::Error;

/// Single attachment cap (mirrors `attachment_bytes` 8 MiB).
pub const ATTACHMENT_CAP_BYTES: usize = 8 * 1024 * 1024;
/// Total attachments cap per request (`attachments_total_bytes` 16 MiB).
pub const ATTACHMENTS_TOTAL_CAP_BYTES: usize = 16 * 1024 * 1024;

/// Typed attachment errors (kinds only, no contents).
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum AttachmentError {
    /// MIME kind outside `text/*` and the advertised image set.
    #[error("unsupported attachment kind")]
    UnsupportedKind,
    /// Bytes do not match the advertised image magic.
    #[error("attachment verification failed")]
    VerificationFailed,
    /// Single or total cap exceeded.
    #[error("attachment too large")]
    TooLarge,
}

/// Verified attachment ready for request encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    /// Advertised MIME type.
    pub mime: String,
    /// Verified raw bytes.
    pub bytes: Vec<u8>,
}

/// Verify `mime` against `bytes` and enforce the single cap.
pub fn verify_attachment(mime: &str, bytes: &[u8]) -> Result<Attachment, AttachmentError> {
    if bytes.len() > ATTACHMENT_CAP_BYTES {
        return Err(AttachmentError::TooLarge);
    }
    if mime.starts_with("text/") {
        if bytes.contains(&0) {
            return Err(AttachmentError::VerificationFailed);
        }
        return Ok(Attachment {
            mime: mime.to_string(),
            bytes: bytes.to_vec(),
        });
    }
    let ok = match mime {
        "image/png" => bytes.starts_with(&[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n']),
        "image/jpeg" => bytes.starts_with(&[0xFF, 0xD8, 0xFF]),
        "image/gif" => bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a"),
        "image/webp" => bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP",
        _ => return Err(AttachmentError::UnsupportedKind),
    };
    if !ok {
        return Err(AttachmentError::VerificationFailed);
    }
    Ok(Attachment {
        mime: mime.to_string(),
        bytes: bytes.to_vec(),
    })
}

/// Encode verified attachments as request input segments under the total cap.
pub fn encode_segments(
    attachments: &[Attachment],
) -> Result<Vec<serde_json::Value>, AttachmentError> {
    let total: usize = attachments.iter().map(|a| a.bytes.len()).sum();
    if total > ATTACHMENTS_TOTAL_CAP_BYTES {
        return Err(AttachmentError::TooLarge);
    }
    Ok(attachments
        .iter()
        .map(|a| {
            use base64::Engine as _;
            let data = base64::engine::general_purpose::STANDARD.encode(&a.bytes);
            serde_json::json!({
                "type": "input_image",
                "image_url": format!("data:{};base64,{data}", a.mime),
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::{AttachmentError, encode_segments, verify_attachment};

    #[test]
    fn prov05_images_text_caps_refusals() {
        let png = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n', 0x00];
        let a = verify_attachment("image/png", &png).expect("png");
        assert_eq!(a.mime, "image/png");
        assert!(matches!(
            verify_attachment("image/png", b"not a png"),
            Err(AttachmentError::VerificationFailed)
        ));
        assert!(matches!(
            verify_attachment("audio/mpeg", b"ID3"),
            Err(AttachmentError::UnsupportedKind)
        ));
        assert!(matches!(
            verify_attachment("application/pdf", b"%PDF"),
            Err(AttachmentError::UnsupportedKind)
        ));
        assert!(matches!(
            verify_attachment("video/mp4", b"ftyp"),
            Err(AttachmentError::UnsupportedKind)
        ));
        verify_attachment("text/plain", b"hello").expect("text");
        assert!(matches!(
            verify_attachment("text/plain", &[0x41, 0x00]),
            Err(AttachmentError::VerificationFailed)
        ));
        let big = vec![b'a'; super::ATTACHMENT_CAP_BYTES + 1];
        assert!(matches!(
            verify_attachment("text/plain", &big),
            Err(AttachmentError::TooLarge)
        ));
        let segs = encode_segments(&[a]).expect("encode");
        assert_eq!(segs.len(), 1);
        assert!(
            segs[0]["image_url"]
                .as_str()
                .expect("url")
                .starts_with("data:image/png;base64,")
        );
    }
}
