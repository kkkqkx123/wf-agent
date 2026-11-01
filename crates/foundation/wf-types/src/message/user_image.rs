//! Structured user image input: the wire payload a client sends alongside
//! the plain-text prompt, plus validation, token budgeting and conversion
//! into conversation content blocks.
//!
//! Text keeps traveling in the `message` string; images travel here so
//! binary payloads never pass through text decoding. The session entry
//! turns validated inputs into `ImageUrl` content blocks (remote address or
//! inline data URI), which the provider codecs already serve.

use llm_types::message::{ImageDetail, ImageUrlContent, MessageContent, MessageContentValue};

/// Media types accepted for inline image data. Kept to the formats every
/// served provider renders; anything else is rejected at the entry.
const ALLOWED_MEDIA_TYPES: [&str; 4] = ["image/png", "image/jpeg", "image/gif", "image/webp"];

/// Upper bound of images carried by one turn.
pub const MAX_USER_IMAGES_PER_TURN: usize = 8;

/// Upper bound of decoded bytes per image (10 MiB).
pub const MAX_USER_IMAGE_BYTES: usize = 10 * 1024 * 1024;

/// Upper bound of decoded bytes across one turn (20 MiB).
pub const MAX_USER_IMAGES_TOTAL_BYTES: usize = 20 * 1024 * 1024;

/// Base token charge per image block, before size scaling.
const IMAGE_TOKEN_BASE: u64 = 85;

/// Bytes covered by one token when scaling image cost.
const IMAGE_TOKEN_BYTES_PER_TOKEN: u64 = 1024;

/// Token estimate for a remote image of unknown size.
const REMOTE_IMAGE_TOKEN_FALLBACK: u64 = 1024;

/// One image attached to a user turn.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UserImageInput {
    /// Inline payload as base64. Empty when `url` carries the image.
    #[serde(default)]
    pub data: String,
    /// IANA media type of `data` (required when `data` is present).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    /// Remote `https` address or `data:` URI. Takes precedence over `data`
    /// when both are set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Render detail hint forwarded to the provider codec.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<ImageDetail>,
}

impl UserImageInput {
    /// Inline base64 image with its media type.
    pub fn inline(data: impl Into<String>, media_type: impl Into<String>) -> Self {
        Self {
            data: data.into(),
            media_type: Some(media_type.into()),
            url: None,
            detail: None,
        }
    }

    /// Remote image address.
    pub fn remote(url: impl Into<String>) -> Self {
        Self {
            data: String::new(),
            media_type: None,
            url: Some(url.into()),
            detail: None,
        }
    }

    /// Whether the entry carries any image source at all.
    pub fn is_empty(&self) -> bool {
        self.data.trim().is_empty() && self.url.as_deref().is_none_or(|url| url.trim().is_empty())
    }
}

/// Media type of an image path by extension. Mirrors the entry whitelist
/// so mention resolution, CLI flags and validation agree on formats.
/// Returns `None` for non-image paths.
pub fn media_type_for_image_path(path: &str) -> Option<&'static str> {
    let extension = std::path::Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

/// Whether a workspace path names an image by extension.
pub fn is_image_path(path: &str) -> bool {
    media_type_for_image_path(path).is_some()
}

/// Decode the inline payload size in bytes. Remote images report `None`
/// because their size is unknown before download.
fn decoded_bytes(image: &UserImageInput) -> Option<usize> {
    if image.data.trim().is_empty() {
        return None;
    }
    base64_decode_len(image.data.trim())
}

/// Inline payload size in decoded bytes, if the image carries inline data.
/// Used for display and budgeting without re-decoding the payload.
pub fn user_image_bytes(image: &UserImageInput) -> Option<usize> {
    decoded_bytes(image)
}

/// Length of a base64 payload in decoded bytes without allocating the
/// decoded form. Returns `None` on malformed input.
fn base64_decode_len(payload: &str) -> Option<usize> {
    let mut sextets = 0usize;
    let mut padding = 0usize;
    for byte in payload.bytes() {
        if byte.is_ascii_whitespace() {
            continue;
        }
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'+' | b'/' => {
                if padding > 0 {
                    return None;
                }
                sextets += 1;
            }
            b'=' => padding += 1,
            _ => return None,
        }
    }
    if padding > 2 || !(sextets + padding).is_multiple_of(4) {
        return None;
    }
    let total = sextets + padding;
    if total == 0 {
        return Some(0);
    }
    Some(total / 4 * 3 - padding)
}

/// Validate one turn of image inputs. Every problem is a hard error: an
/// oversized or mistyped image must never degrade into a silent drop.
pub fn validate_user_images(images: &[UserImageInput]) -> Result<(), String> {
    if images.len() > MAX_USER_IMAGES_PER_TURN {
        return Err(format!(
            "too many images: {} exceed the {MAX_USER_IMAGES_PER_TURN}-image per-turn limit",
            images.len()
        ));
    }
    let mut total_bytes = 0usize;
    for (index, image) in images.iter().enumerate() {
        let field = format!("images[{index}]");
        if image.is_empty() {
            return Err(format!("{field} carries no image data or url"));
        }
        if let Some(url) = image
            .url
            .as_deref()
            .map(str::trim)
            .filter(|url| !url.is_empty())
        {
            if !(url.starts_with("https://")
                || url.starts_with("http://")
                || url.starts_with("data:"))
            {
                return Err(format!(
                    "{field} url must use https, http or data, got a different scheme"
                ));
            }
            continue;
        }
        let media = image
            .media_type
            .as_deref()
            .map(str::trim)
            .unwrap_or_default();
        if media.is_empty() {
            return Err(format!("{field} needs a media_type for inline data"));
        }
        if !ALLOWED_MEDIA_TYPES.contains(&media) {
            return Err(format!(
                "{field} media type '{media}' is not served; use one of {}",
                ALLOWED_MEDIA_TYPES.join(", ")
            ));
        }
        let Some(bytes) = decoded_bytes(image) else {
            return Err(format!("{field} data is not valid base64"));
        };
        if bytes == 0 {
            return Err(format!("{field} data is empty"));
        }
        if bytes > MAX_USER_IMAGE_BYTES {
            return Err(format!(
                "{field} holds {} bytes, exceeding the per-image limit of {MAX_USER_IMAGE_BYTES} bytes",
                bytes
            ));
        }
        total_bytes += bytes;
    }
    if total_bytes > MAX_USER_IMAGES_TOTAL_BYTES {
        return Err(format!(
            "images hold {total_bytes} bytes in total, exceeding the per-turn limit of {MAX_USER_IMAGES_TOTAL_BYTES} bytes"
        ));
    }
    Ok(())
}

/// Token budget of one image input: a base charge plus one token per
/// kilobyte of payload. Remote images of unknown size take the fallback.
pub fn estimate_user_image_tokens(image: &UserImageInput) -> u64 {
    if let Some(url) = image
        .url
        .as_deref()
        .map(str::trim)
        .filter(|url| !url.is_empty())
    {
        if let Some(rest) = url.strip_prefix("data:") {
            if let Some(payload) = rest.split_once(',').map(|(_, data)| data) {
                let bytes = base64_decode_len(payload).unwrap_or(0) as u64;
                return IMAGE_TOKEN_BASE + bytes / IMAGE_TOKEN_BYTES_PER_TOKEN;
            }
        }
        return REMOTE_IMAGE_TOKEN_FALLBACK;
    }
    let bytes = decoded_bytes(image).unwrap_or(0) as u64;
    IMAGE_TOKEN_BASE + bytes / IMAGE_TOKEN_BYTES_PER_TOKEN
}

/// Token budget of a stored `ImageUrl` block. Data URIs scale with their
/// payload; remote addresses and file references take the fallback.
pub fn estimate_image_block_tokens(url: &str) -> u64 {
    estimate_user_image_tokens(&UserImageInput::remote(url))
}

/// Convert a validated image input into its conversation content block.
/// Remote addresses travel verbatim; inline data becomes a `data:` URI so
/// the provider codecs receive the bytes they already serve.
pub fn to_image_block(image: &UserImageInput) -> MessageContent {
    if let Some(url) = image
        .url
        .as_deref()
        .map(str::trim)
        .filter(|url| !url.is_empty())
    {
        return MessageContent::ImageUrl {
            image_url: ImageUrlContent {
                url: url.to_string(),
                detail: image.detail,
                file_id: None,
            },
        };
    }
    let media = image
        .media_type
        .as_deref()
        .map(str::trim)
        .unwrap_or_default();
    MessageContent::ImageUrl {
        image_url: ImageUrlContent {
            url: format!("data:{media};base64,{}", image.data.trim()),
            detail: image.detail,
            file_id: None,
        },
    }
}

/// Placeholder line for an image block in text-only views (session tree,
/// replay, search snippets). Keeps image-carrying turns visible where no
/// image renderer exists.
pub fn image_block_label(image_url: &ImageUrlContent) -> String {
    if let Some(rest) = image_url.url.strip_prefix("data:") {
        let media = rest.split(';').next().unwrap_or("image");
        return format!("[image: {media} inline]");
    }
    if image_url.url.trim().is_empty() {
        return "[image]".to_string();
    }
    format!("[image: {}]", image_url.url.trim())
}

/// Conversation content of a user turn: plain text when no images ride
/// along, rich blocks otherwise. Returns `None` when the turn carries
/// nothing at all, so callers keep a single emptiness check.
pub fn user_content(text: &str, images: &[UserImageInput]) -> Option<MessageContentValue> {
    if images.is_empty() {
        return if text.is_empty() {
            None
        } else {
            Some(MessageContentValue::Text(text.to_string()))
        };
    }
    let mut blocks = Vec::with_capacity(images.len() + 1);
    if !text.is_empty() {
        blocks.push(MessageContent::Text {
            text: text.to_string(),
        });
    }
    blocks.extend(images.iter().map(to_image_block));
    if blocks.is_empty() {
        return None;
    }
    Some(MessageContentValue::Rich(blocks))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inline_image(payload: &str) -> UserImageInput {
        UserImageInput::inline(payload.to_string(), "image/png")
    }

    #[test]
    fn valid_inputs_pass() {
        let images = vec![
            inline_image("aGk="),
            UserImageInput::remote("https://x/y.png"),
        ];
        assert!(validate_user_images(&images).is_ok());
        assert!(validate_user_images(&[]).is_ok());
    }

    #[test]
    fn entry_limits_reject() {
        assert!(validate_user_images(&[UserImageInput::inline("", "image/png")]).is_err());
        assert!(validate_user_images(&[UserImageInput {
            data: String::new(),
            media_type: None,
            url: None,
            detail: None,
        }])
        .is_err());
        let too_many =
            vec![UserImageInput::remote("https://x/y.png"); MAX_USER_IMAGES_PER_TURN + 1];
        assert!(validate_user_images(&too_many).is_err());
    }

    #[test]
    fn media_type_and_payload_are_checked() {
        assert!(validate_user_images(&[UserImageInput::inline("aGk=", "image/tiff")]).is_err());
        assert!(validate_user_images(&[UserImageInput {
            data: "aGk=".to_string(),
            media_type: None,
            url: None,
            detail: None,
        }])
        .is_err());
        assert!(validate_user_images(&[inline_image("!!!")]).is_err());
    }

    #[test]
    fn url_schemes_are_checked() {
        assert!(validate_user_images(&[UserImageInput::remote("ftp://x/y.png")]).is_err());
        assert!(
            validate_user_images(&[UserImageInput::remote("data:image/png;base64,aGk=")]).is_ok()
        );
    }

    #[test]
    fn user_content_shapes_match_payload() {
        assert!(user_content("", &[]).is_none());
        assert!(matches!(
            user_content("hi", &[]),
            Some(MessageContentValue::Text(_))
        ));
        let rich = user_content("", &[inline_image("aGk=")]);
        let MessageContentValue::Rich(blocks) = rich.expect("rich content") else {
            panic!("expected rich blocks");
        };
        assert_eq!(blocks.len(), 1);
        let rich = user_content("see", &[inline_image("aGk=")]);
        let MessageContentValue::Rich(blocks) = rich.expect("rich content") else {
            panic!("expected rich blocks");
        };
        assert_eq!(blocks.len(), 2);
    }

    #[test]
    fn image_paths_detect_by_extension() {
        assert!(is_image_path("shot.PNG"));
        assert!(is_image_path("a/b/photo.jpg"));
        assert!(!is_image_path("notes.md"));
        assert!(!is_image_path("noext"));
    }
}
