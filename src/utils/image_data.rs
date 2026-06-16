use crate::infra::s3::S3Storage;
use crate::utils::awnser::photos_are_similar;

pub async fn compare_step_photos(
    reference: &str,
    submitted: &str,
    s3: &S3Storage,
    threshold: u32,
) -> anyhow::Result<bool> {
    let reference_bytes = s3.read_stored_image(reference).await?;
    let submitted_bytes = resolve_submitted_bytes(submitted, s3).await?;
    Ok(photos_are_similar(
        &submitted_bytes,
        &reference_bytes,
        threshold,
    ))
}

async fn resolve_submitted_bytes(value: &str, s3: &S3Storage) -> anyhow::Result<Vec<u8>> {
    let trimmed = value.trim();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return s3.read_stored_image(trimmed).await;
    }
    if trimmed.contains('/') && !looks_like_base64(trimmed) {
        return s3.read_stored_image(trimmed).await;
    }
    decode_submitted_photo(trimmed)
}

fn looks_like_base64(value: &str) -> bool {
    let sample = value.chars().take(64).collect::<String>();
    !sample.is_empty() && sample.chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=')
}

fn decode_submitted_photo(value: &str) -> anyhow::Result<Vec<u8>> {
    use base64::{prelude::BASE64_STANDARD, Engine};

    let payload = if let Some(idx) = value.find(',') {
        value[idx + 1..].trim()
    } else {
        value.trim()
    };

    BASE64_STANDARD
        .decode(payload)
        .map_err(|_| anyhow::anyhow!("invalid photo data"))
}
