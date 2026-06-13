use crate::infra::s3::S3Storage;
use crate::utils::awnser::photos_are_similar;

pub async fn compare_step_photos(
    reference: &str,
    submitted: &str,
    s3: &S3Storage,
    threshold: u32,
) -> anyhow::Result<bool> {
    let reference_bytes = s3.fetch_bytes(reference).await?;
    let submitted_bytes = decode_submitted_photo(submitted)?;
    Ok(photos_are_similar(
        &submitted_bytes,
        &reference_bytes,
        threshold,
    ))
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
