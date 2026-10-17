use super::*;
use crate::provider::{InputContent, InputItem, InputRole};
use base64::Engine as _;
use oc_core::prompt::{ImageMark, PromptImage, PromptInput, UserImages};

pub(crate) fn prepare_prompt(
    prompt: &PromptInput,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<InputItem, AttachmentError> {
    prompt
        .check_budget()
        .map_err(|_| AttachmentError::TooLarge)?;
    if prompt.text.len() > oc_core::session::MAX_INPUT_BYTES
        || prompt
            .images
            .iter()
            .try_fold(0usize, |n, i| n.checked_add(i.png.len()))
            .is_none_or(|n| n > ATTACHMENTS_TOTAL_CAP_BYTES)
    {
        return Err(AttachmentError::TooLarge);
    }
    let marks: Vec<_> = prompt.images.iter().map(|i| i.mark.clone()).collect();
    validate_marks(&prompt.text, &marks)?;
    let mut content = Vec::new();
    let mut start = 0;
    for image in &prompt.images {
        validate_png(&image.png, cancel)?;
        content.push(InputContent::InputText {
            text: prompt.text[start..image.mark.end].into(),
        });
        content.push(InputContent::InputImage {
            image_url: format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(&image.png)
            ),
            detail: None,
        });
        start = image.mark.end;
    }
    if start < prompt.text.len() || content.is_empty() {
        content.push(InputContent::InputText {
            text: prompt.text[start..].into(),
        });
    }
    Ok(InputItem::Message {
        role: InputRole::User,
        content,
    })
}

fn validate_marks(text: &str, marks: &[ImageMark]) -> Result<(), AttachmentError> {
    if marks.len() > oc_core::prompt::MAX_IMAGES {
        return Err(AttachmentError::TooLarge);
    }
    let mut end = 0;
    let mut ordinals = std::collections::BTreeSet::new();
    for mark in marks {
        if mark.start < end
            || mark.start >= mark.end
            || mark.ordinal == 0
            || !ordinals.insert(mark.ordinal)
            || text.get(mark.start..mark.end) != Some(mark.label().as_str())
        {
            return Err(AttachmentError::VerificationFailed);
        }
        end = mark.end;
    }
    Ok(())
}

pub(crate) fn image_metadata(
    text: &str,
    value: &serde_json::Value,
) -> Result<Option<UserImages>, AttachmentError> {
    let Some(value) = value.get("user_images") else {
        return Ok(None);
    };
    let metadata: UserImages =
        serde_json::from_value(value.clone()).map_err(|_| AttachmentError::VerificationFailed)?;
    validate_marks(text, &metadata.images)?;
    Ok(Some(metadata))
}

pub(crate) fn restore_prompt(
    text: String,
    metadata: &UserImages,
    item: &InputItem,
) -> Result<PromptInput, AttachmentError> {
    validate_marks(&text, &metadata.images)?;
    let InputItem::Message {
        role: InputRole::User,
        content,
    } = item
    else {
        return Err(AttachmentError::VerificationFailed);
    };
    let urls: Vec<_> = content
        .iter()
        .filter_map(|part| match part {
            InputContent::InputImage { image_url, .. } => Some(image_url),
            _ => None,
        })
        .collect();
    let original: String = content
        .iter()
        .filter_map(|part| match part {
            InputContent::InputText { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    if original != text || urls.len() != metadata.images.len() {
        return Err(AttachmentError::VerificationFailed);
    }
    let mut images = Vec::new();
    let mut total = 0usize;
    for (url, mark) in urls.into_iter().zip(&metadata.images) {
        let data = url
            .strip_prefix("data:image/png;base64,")
            .ok_or(AttachmentError::VerificationFailed)?;
        if data.len() > ATTACHMENT_CAP_BYTES.div_ceil(3) * 4 {
            return Err(AttachmentError::TooLarge);
        }
        let png = base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|_| AttachmentError::VerificationFailed)?;
        total = total
            .checked_add(png.len())
            .ok_or(AttachmentError::TooLarge)?;
        if total > ATTACHMENTS_TOTAL_CAP_BYTES {
            return Err(AttachmentError::TooLarge);
        }
        validate_png(&png, &std::sync::atomic::AtomicBool::new(false))?;
        images.push(PromptImage {
            mark: mark.clone(),
            png: png.into(),
        });
    }
    Ok(PromptInput { text, images })
}
