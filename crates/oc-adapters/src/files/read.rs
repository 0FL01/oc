//! One descriptor-held read owner: bounded text, directory pages and image bytes.
use super::search::Budget;
use super::*;
use std::sync::atomic::AtomicBool;

pub(crate) enum Content {
    Text(ReadResult),
    Directory(ReadResult),
    Image {
        bytes: Vec<u8>,
        mime: &'static str,
        source_path: PathBuf,
    },
}

impl Files {
    pub(crate) fn read_admitted(
        &self,
        path: &str,
        offset: u64,
        limit: usize,
        allowed: impl Fn(&Path) -> bool,
        cancel: &AtomicBool,
    ) -> Result<Content, FileToolError> {
        let budget = Budget::new(Some(cancel));
        budget.check()?;
        let abs = self.resolve_read(path)?;
        let data = std::fs::canonicalize(&self.data_root)
            .unwrap_or_else(|_| normalize_suggest_root(&self.data_root));
        if abs.starts_with(&data) {
            return Err(FileToolError::OwnDataRoot);
        }
        if !allowed(&abs) {
            return Err(FileToolError::InvalidPattern("read scope denied".into()));
        }
        budget.check()?;
        let mut file = self.open_read_scope(&abs, libc::O_RDONLY)?;
        budget.check()?;
        let meta = file.metadata().map_err(|_| FileToolError::Io)?;
        if meta.is_dir() {
            let mut inspected = 0;
            let mut truncated = false;
            let names = suggest_dir_names(
                &file,
                &mut inspected,
                &mut truncated,
                WALK_FILES_CAP,
                || budget.check(),
            )?;
            if truncated {
                return Err(FileToolError::BudgetExhausted);
            }
            let mut entries = Vec::new();
            for name in names {
                budget.check()?;
                let child = abs.join(&name);
                if child.starts_with(&data) || !allowed(&child) {
                    continue;
                }
                // O_PATH obtains only entry metadata; symlinks are never followed.
                let name_c = CString::new(name.as_bytes()).map_err(|_| FileToolError::Io)?;
                let entry =
                    suggest_openat(&file, &name_c, libc::O_PATH).map_err(|_| FileToolError::Io)?;
                let meta = entry.metadata().map_err(|_| FileToolError::Io)?;
                if !meta.is_file() && !meta.is_dir() && !meta.file_type().is_symlink() {
                    continue;
                }
                let name = name
                    .to_str()
                    .filter(|s| !s.chars().any(char::is_control))
                    .ok_or_else(|| {
                        FileToolError::InvalidPattern(
                            "directory has unrepresentable entry names".into(),
                        )
                    })?;
                entries.push((
                    !meta.is_dir(),
                    format!("{name}{}", if meta.is_dir() { "/" } else { "" }),
                ));
            }
            entries.sort();
            budget.check()?;
            return page(entries.iter().map(|(_, s)| s.as_str()), offset, limit)
                .map(Content::Directory);
        }
        if !meta.is_file() {
            return Err(FileToolError::InvalidPattern(
                "read requires a regular file or directory".into(),
            ));
        }
        if meta.len() > GREP_FILE_BYTES_CAP {
            return Err(FileToolError::BudgetExhausted);
        }
        let mut bytes = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            budget.check()?;
            let n = file.read(&mut chunk).map_err(|_| FileToolError::Io)?;
            budget.check()?;
            if n == 0 {
                break;
            }
            if bytes.len() + n > GREP_FILE_BYTES_CAP as usize {
                return Err(FileToolError::BudgetExhausted);
            }
            bytes.extend_from_slice(&chunk[..n]);
            #[cfg(test)]
            super::search::read_checkpoint(self, cancel)?;
        }
        if bytes.starts_with(b"%PDF-")
            || Path::new(path)
                .extension()
                .is_some_and(|s| s.eq_ignore_ascii_case("pdf"))
        {
            return Err(FileToolError::InvalidPattern(
                "PDF is unsupported; read a text export instead".into(),
            ));
        }
        if let Ok(format) = image::guess_format(&bytes) {
            let mime = match format {
                image::ImageFormat::Png => "image/png",
                image::ImageFormat::Jpeg => "image/jpeg",
                image::ImageFormat::Gif => "image/gif",
                image::ImageFormat::WebP => "image/webp",
                _ => {
                    return Err(FileToolError::InvalidPattern(
                        "unsupported image format; use PNG, JPEG, GIF or WebP".into(),
                    ));
                }
            };
            validate_image_with_budget(&bytes, format, &budget)?;
            return Ok(Content::Image {
                bytes,
                mime,
                source_path: abs,
            });
        }
        if Path::new(path).extension().is_some_and(|s| {
            ["png", "jpg", "jpeg", "gif", "webp"]
                .iter()
                .any(|e| s.eq_ignore_ascii_case(e))
        }) {
            return Err(FileToolError::InvalidPattern(
                "invalid image; provide a complete PNG, JPEG, GIF or WebP".into(),
            ));
        }
        if bytes.contains(&0) {
            return Err(FileToolError::Binary);
        }
        let text = std::str::from_utf8(&bytes).map_err(|_| FileToolError::Binary)?;
        let page = page(text.lines(), offset, limit).map(Content::Text);
        budget.check()?;
        page
    }
}

fn page<'a>(
    lines: impl Iterator<Item = &'a str>,
    offset: u64,
    limit: usize,
) -> Result<ReadResult, FileToolError> {
    let mut source = lines
        .skip(usize::try_from(offset - 1).map_err(|_| FileToolError::BudgetExhausted)?)
        .peekable();
    if offset != 1 && source.peek().is_none() {
        return Err(FileToolError::InvalidPattern(
            "offset is out of range".into(),
        ));
    }
    // Reserve header/cursor/line references in the existing rendered-page ceiling.
    let mut used = 8192;
    let mut selected = Vec::new();
    let mut line_truncated = false;
    while selected.len() < limit {
        let Some(line) = source.peek() else {
            break;
        };
        let line_bytes = line.len() + (offset + selected.len() as u64).to_string().len() + 3;
        if used + line_bytes > READ_BYTES_CAP {
            if selected.is_empty() {
                let line = source.next().expect("peeked line");
                let mut end = READ_BYTES_CAP - used - 128;
                while !line.is_char_boundary(end) {
                    end -= 1;
                }
                selected.push(format!(
                    "{} [Line truncated; use grep for a bounded preview]",
                    &line[..end]
                ));
                line_truncated = true;
            }
            break;
        }
        used += line_bytes;
        selected.push(source.next().expect("peeked line").into());
    }
    let truncated = source.peek().is_some();
    Ok(ReadResult {
        offset,
        next_offset: truncated.then_some(offset + selected.len() as u64),
        lines: selected,
        truncated: truncated || line_truncated,
    })
}

// A cancellation-aware reader is borrowed by the decoder, never by a worker
// detached from its invocation. Decoder allocations use the existing attachment cap.
struct ImageBytes<'a> {
    inner: std::io::Cursor<&'a [u8]>,
    budget: &'a Budget<'a>,
}
impl std::io::Read for ImageBytes<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.budget.check().is_err() {
            return Err(std::io::ErrorKind::Other.into());
        }
        let length = buf.len().min(8192);
        self.inner.read(&mut buf[..length])
    }
}
impl std::io::Seek for ImageBytes<'_> {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        self.budget
            .check()
            .map_err(|_| std::io::Error::from(std::io::ErrorKind::Other))?;
        self.inner.seek(pos)
    }
}
pub(crate) fn validate_image(
    bytes: &[u8],
    format: image::ImageFormat,
    cancel: &AtomicBool,
) -> Result<(), FileToolError> {
    validate_image_with_budget(bytes, format, &Budget::new(Some(cancel)))
}

fn validate_image_with_budget(
    bytes: &[u8],
    format: image::ImageFormat,
    budget: &Budget<'_>,
) -> Result<(), FileToolError> {
    use image::{AnimationDecoder, ImageDecoder};
    budget.check()?;
    // Decoder pixel success alone may omit the container's terminal record.
    // Require a complete envelope as well; pixel/animation parsing remains
    // exclusively the vetted decoder's responsibility.
    let complete = match format {
        image::ImageFormat::Png => bytes.ends_with(b"\0\0\0\0IEND\xaeB`\x82"),
        image::ImageFormat::Jpeg => bytes.ends_with(b"\xff\xd9"),
        image::ImageFormat::Gif => bytes.ends_with(b";"),
        image::ImageFormat::WebP => bytes.get(4..8).is_some_and(|size| {
            u32::from_le_bytes(size.try_into().expect("four bytes")) as usize + 8 == bytes.len()
        }),
        _ => false,
    };
    if !complete {
        return Err(FileToolError::InvalidPattern(
            "incomplete image container; provide a complete bounded image".into(),
        ));
    }
    let reader = || {
        std::io::BufReader::new(ImageBytes {
            inner: std::io::Cursor::new(bytes),
            budget,
        })
    };
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(crate::attachments::ATTACHMENT_CAP_BYTES as u64);
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    let decoded = (|| -> image::ImageResult<()> {
        match format {
            image::ImageFormat::Gif => {
                let mut decoder = image::codecs::gif::GifDecoder::new(reader())?;
                decoder.set_limits(limits)?;
                validate_frames(decoder.into_frames(), budget)?;
            }
            image::ImageFormat::Png => {
                let decoder = image::codecs::png::PngDecoder::with_limits(reader(), limits)?;
                if decoder.is_apng()? {
                    validate_frames(decoder.apng()?.into_frames(), budget)?;
                } else {
                    validate_static(decoder)?;
                }
            }
            image::ImageFormat::WebP => {
                let mut decoder = image::codecs::webp::WebPDecoder::new(reader())?;
                decoder.set_limits(limits)?;
                // The animation iterator allocates RGBA canvases independently
                // of the static decoder; bound those before constructing frames.
                let (w, h) = decoder.dimensions();
                if u64::from(w) * u64::from(h) * 4 > crate::attachments::ATTACHMENT_CAP_BYTES as u64
                {
                    return Err(image::ImageError::IoError(
                        std::io::ErrorKind::OutOfMemory.into(),
                    ));
                }
                if decoder.has_animation() {
                    validate_frames(decoder.into_frames(), budget)?;
                } else {
                    validate_static(decoder)?;
                }
            }
            _ => {
                let mut decoder = image::ImageReader::with_format(reader(), format);
                decoder.limits(limits);
                decoder.decode()?;
            }
        }
        Ok(())
    })();
    budget.check()?;
    decoded.map_err(|_| {
        FileToolError::InvalidPattern(
            "invalid or over-budget image; provide a complete bounded image".into(),
        )
    })?;
    Ok(())
}

fn validate_static(decoder: impl image::ImageDecoder) -> image::ImageResult<()> {
    if decoder.total_bytes() > crate::attachments::ATTACHMENT_CAP_BYTES as u64 {
        return Err(image::ImageError::IoError(
            std::io::ErrorKind::OutOfMemory.into(),
        ));
    }
    image::DynamicImage::from_decoder(decoder)?;
    Ok(())
}

fn validate_frames(mut frames: image::Frames<'_>, budget: &Budget<'_>) -> image::ImageResult<()> {
    let mut decoded = 0usize;
    let mut count = 0;
    loop {
        if budget.check().is_err() {
            return Err(image::ImageError::IoError(std::io::ErrorKind::Other.into()));
        }
        let Some(frame) = frames.next() else {
            break;
        };
        decoded = decoded.saturating_add(frame?.buffer().len());
        if decoded > crate::attachments::ATTACHMENT_CAP_BYTES {
            return Err(image::ImageError::IoError(
                std::io::ErrorKind::OutOfMemory.into(),
            ));
        }
        count += 1;
    }
    if count == 0 {
        return Err(image::ImageError::IoError(
            std::io::ErrorKind::UnexpectedEof.into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
