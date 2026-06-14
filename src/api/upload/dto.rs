use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadImageResp {
    pub url: String,
    /// Stable S3 object key — prefer storing this over the public URL when persisting references.
    pub key: String,
}
