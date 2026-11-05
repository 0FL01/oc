//! Bounded verified attachments for T13 (PROV05).
//!
//! Advertised `text`/`image` kinds only: images are verified by magic bytes
//! (PNG/JPEG/GIF/WEBP), single and total caps enforced before encoding, and
//! audio/video/PDF fail visibly. Encoded request segments stay under the
//! request cap; the model never smuggles credentials through attachments.

use thiserror::Error;

mod image_budget;
mod projection;
mod prompt;
pub use image_budget::ImageSource;
pub(crate) use image_budget::{defer_images, image_measurement, project_images};
pub(crate) use projection::{omitted_images, project_media};
pub(crate) use prompt::{image_metadata, prepare_prompt, restore_prompt};

/// Explicit CLI file inputs use the existing text/PNG prompt and own-data guard.
/// Supported image containers normalize to PNG; PDF remains unsupported.
/// No filesystem reference or new attachment kind crosses the native protocol.
pub fn attach_files(
    mut prompt: oc_core::prompt::PromptInput,
    paths: &[std::path::PathBuf],
    project: &std::path::Path,
    data: &std::path::Path,
) -> Result<oc_core::prompt::PromptInput, AttachmentError> {
    use std::io::Read as _;
    use std::os::unix::fs::OpenOptionsExt as _;
    const FILE_BYTES: usize = 10 * 1024 * 1024;
    if paths.len() > 100 {
        return Err(AttachmentError::TooLarge);
    }
    let scope =
        crate::files::Files::new(project, data).map_err(|_| AttachmentError::VerificationFailed)?;
    let mut images = Vec::new();
    let mut image_bytes = 0usize;
    for path in paths {
        let target = scope
            .io_target(&project.join(path))
            .map_err(|_| AttachmentError::VerificationFailed)?;
        let file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(target)
            .map_err(|_| AttachmentError::VerificationFailed)?;
        scope
            .check_data_file(&file)
            .map_err(|_| AttachmentError::VerificationFailed)?;
        let meta = file
            .metadata()
            .map_err(|_| AttachmentError::VerificationFailed)?;
        if !meta.is_file() {
            return Err(AttachmentError::UnsupportedKind);
        }
        if meta.len() > FILE_BYTES as u64 {
            return Err(AttachmentError::TooLarge);
        }
        let mut bytes = Vec::new();
        file.take((FILE_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| AttachmentError::VerificationFailed)?;
        if bytes.len() > FILE_BYTES {
            return Err(AttachmentError::TooLarge);
        }
        let extension = path
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_ascii_lowercase();
        let format = image::guess_format(&bytes).ok().filter(|format| {
            matches!(
                format,
                image::ImageFormat::Png
                    | image::ImageFormat::Jpeg
                    | image::ImageFormat::Gif
                    | image::ImageFormat::WebP
            )
        });
        if let Some(format) = format {
            crate::files::read::validate_image(
                &bytes,
                format,
                &std::sync::atomic::AtomicBool::new(false),
            )
            .map_err(|_| AttachmentError::VerificationFailed)?;
            if format != image::ImageFormat::Png {
                let mut reader =
                    image::ImageReader::with_format(std::io::Cursor::new(&bytes), format);
                let mut limits = image::Limits::default();
                limits.max_alloc = Some(ATTACHMENT_CAP_BYTES as u64);
                limits.max_image_width = Some(4096);
                limits.max_image_height = Some(4096);
                reader.limits(limits);
                let decoded = reader
                    .decode()
                    .map_err(|_| AttachmentError::VerificationFailed)?;
                let mut png = std::io::Cursor::new(Vec::new());
                decoded
                    .write_to(&mut png, image::ImageFormat::Png)
                    .map_err(|_| AttachmentError::VerificationFailed)?;
                bytes = png.into_inner();
            }
            image_bytes += bytes.len();
            if image_bytes > ATTACHMENTS_TOTAL_CAP_BYTES {
                return Err(AttachmentError::TooLarge);
            }
            images.push(bytes);
        } else if matches!(
            extension.as_str(),
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "pdf"
        ) {
            return Err(AttachmentError::UnsupportedKind);
        } else {
            let text = std::str::from_utf8(&bytes).map_err(|_| AttachmentError::UnsupportedKind)?;
            if bytes.contains(&0)
                || bytes
                    .iter()
                    .filter(|&&byte| byte < 9 || (byte > 13 && byte < 32))
                    .count()
                    * 10
                    > bytes.len() * 3
            {
                return Err(AttachmentError::UnsupportedKind);
            }
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            let block = format!("<file name=\"{name}\">\n{text}\n</file>");
            if !prompt.text.is_empty() {
                prompt.text.push_str("\n\n");
            }
            prompt.text.push_str(&block);
            prompt
                .check_budget()
                .map_err(|_| AttachmentError::TooLarge)?;
        }
    }
    for bytes in images {
        if !prompt.text.is_empty() {
            prompt.text.push('\n');
        }
        let ordinal = prompt.images.len() + 1;
        let start = prompt.text.len();
        prompt.text.push_str(&format!("[Image {ordinal}]"));
        prompt.images.push(oc_core::prompt::PromptImage {
            mark: oc_core::prompt::ImageMark {
                start,
                end: prompt.text.len(),
                ordinal,
            },
            png: bytes.into(),
        });
        prompt
            .check_budget()
            .map_err(|_| AttachmentError::TooLarge)?;
    }
    Ok(prompt)
}

/// Full bounded PNG verification shared by clipboard acquisition and admission.
pub fn validate_png(
    bytes: &[u8],
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<(), AttachmentError> {
    if bytes.len() > ATTACHMENT_CAP_BYTES {
        return Err(AttachmentError::TooLarge);
    }
    crate::files::read::validate_image(bytes, image::ImageFormat::Png, cancel)
        .map_err(|_| AttachmentError::VerificationFailed)
}

/// Single attachment cap (mirrors `attachment_bytes` 20 MiB).
pub const ATTACHMENT_CAP_BYTES: usize = oc_core::prompt::IMAGE_BYTES;
/// Total attachments cap per request (`attachments_total_bytes` 40 MiB).
pub const ATTACHMENTS_TOTAL_CAP_BYTES: usize = oc_core::prompt::IMAGES_TOTAL_BYTES;

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
    fn w10_cli_files_capture_text_and_refuse_special_and_own_data_inputs() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        let data = root.path().join("data");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(project.join("context.txt"), "file body\n").unwrap();
        let prompt =
            super::attach_files("prompt".into(), &["context.txt".into()], &project, &data).unwrap();
        assert_eq!(
            prompt.text,
            "prompt\n\n<file name=\"context.txt\">\nfile body\n\n</file>"
        );
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(1, 1)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        std::fs::write(project.join("image.png"), png.get_ref()).unwrap();
        let image =
            super::attach_files("prompt".into(), &["image.png".into()], &project, &data).unwrap();
        assert_eq!(image.text, "prompt\n[Image 1]");
        assert_eq!(&*image.images[0].png, png.get_ref());
        let mut jpeg = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(1, 1)
            .write_to(&mut jpeg, image::ImageFormat::Jpeg)
            .unwrap();
        std::fs::write(project.join("image.jpg"), jpeg.get_ref()).unwrap();
        let normalized =
            super::attach_files("prompt".into(), &["image.jpg".into()], &project, &data).unwrap();
        assert_eq!(normalized.text, "prompt\n[Image 1]");
        super::validate_png(
            &normalized.images[0].png,
            &std::sync::atomic::AtomicBool::new(false),
        )
        .unwrap();
        assert!(
            super::attach_files(
                "prompt".into(),
                std::slice::from_ref(&project),
                &project,
                &data
            )
            .is_err()
        );
        std::fs::write(data.join("private"), "never attached").unwrap();
        std::os::unix::fs::symlink(data.join("private"), project.join("alias")).unwrap();
        assert!(super::attach_files("prompt".into(), &["alias".into()], &project, &data).is_err());
    }

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
