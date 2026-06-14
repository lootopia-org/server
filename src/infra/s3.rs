use aws_config::BehaviorVersion;
use aws_credential_types::provider::SharedCredentialsProvider;
use aws_credential_types::Credentials;
use aws_sdk_s3::config::Region;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::types::ObjectCannedAcl;
use aws_sdk_s3::Client;
use uuid::Uuid;

use crate::config::S3Config;

#[derive(Clone)]
pub struct S3Storage {
    client: Client,
    bucket: String,
    key_prefix: String,
    public_base_url: String,
    endpoint: Option<String>,
    public_reads: bool,
}

impl S3Storage {
    pub async fn connect(cfg: &S3Config) -> anyhow::Result<Self> {
        let public_reads = cfg.endpoint.is_some();
        let client = if let Some(endpoint) = &cfg.endpoint {
            let creds = Credentials::new(
                cfg.access_key_id.clone(),
                cfg.secret_access_key.clone(),
                None,
                None,
                "lootopia-s3",
            );
            Client::from_conf(
                aws_sdk_s3::config::Builder::new()
                    .behavior_version(BehaviorVersion::latest())
                    .region(Region::new(cfg.region.clone()))
                    .endpoint_url(endpoint)
                    .force_path_style(true)
                    .credentials_provider(SharedCredentialsProvider::new(creds))
                    .build(),
            )
        } else {
            let aws_cfg = aws_config::defaults(BehaviorVersion::latest())
                .region(Region::new(cfg.region.clone()))
                .load()
                .await;
            Client::from_conf(aws_sdk_s3::config::Builder::from(&aws_cfg).build())
        };

        let public_base_url = cfg
            .public_base_url
            .clone()
            .unwrap_or_else(|| default_public_base_url(cfg));

        let storage = Self {
            client,
            bucket: cfg.bucket.clone(),
            key_prefix: normalize_prefix(&cfg.key_prefix),
            public_base_url: public_base_url.trim_end_matches('/').to_string(),
            endpoint: cfg.endpoint.clone(),
            public_reads,
        };

        if let Err(err) = storage.ensure_bucket().await {
            tracing::warn!(
                error = %err,
                bucket = %storage.bucket,
                "S3 bucket is unavailable; server will start but image uploads may fail"
            );
        }
        Ok(storage)
    }

    async fn ensure_bucket(&self) -> anyhow::Result<()> {
        if self.endpoint.is_none() {
            tracing::info!("S3 custom endpoint not configured; skipping bucket provisioning");
            return Ok(());
        }

        match self.client.head_bucket().bucket(&self.bucket).send().await {
            Ok(_) => return Ok(()),
            Err(err) => {
                let not_found = err
                    .as_service_error()
                    .is_some_and(|service_err| service_err.is_not_found());
                if !not_found {
                    return Err(anyhow::anyhow!("failed to check s3 bucket: {err}"));
                }
            }
        }

        self.client
            .create_bucket()
            .bucket(&self.bucket)
            .send()
            .await
            .map_err(|err| anyhow::anyhow!("failed to create s3 bucket: {err}"))?;

        Ok(())
    }

    pub fn public_url(&self, key: &str) -> String {
        format!("{}/{}", self.public_base_url, key)
    }

    pub async fn upload_image(
        &self,
        folder: &str,
        data: Vec<u8>,
        content_type: &str,
    ) -> anyhow::Result<String> {
        let ext = extension_for_content_type(content_type);
        let key = format!("{}/{}/{}.{}", self.key_prefix, folder, Uuid::new_v4(), ext);

        let mut request = self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(&key)
            .body(ByteStream::from(data))
            .content_type(content_type);

        if self.public_reads {
            request = request.acl(ObjectCannedAcl::PublicRead);
        }

        request
            .send()
            .await
            .map_err(|err| anyhow::anyhow!("failed to upload to s3: {err}"))?;

        Ok(self.public_url(&key))
    }

    pub async fn fetch_bytes(&self, reference: &str) -> anyhow::Result<Vec<u8>> {
        let trimmed = reference.trim();
        if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            if let Some(key) = self.key_from_url(trimmed) {
                return self.get_object_bytes(&key).await;
            }
            return fetch_http_bytes(trimmed).await;
        }
        decode_base64_bytes(trimmed)
    }

    pub fn key_from_url(&self, url: &str) -> Option<String> {
        if let Some(rest) = url.strip_prefix(&format!("{}/", self.public_base_url)) {
            return Some(rest.to_string());
        }

        if let Some(endpoint) = &self.endpoint {
            let path_prefix = format!(
                "{}/{}/",
                endpoint.trim_end_matches('/'),
                self.bucket
            );
            if let Some(rest) = url.strip_prefix(&path_prefix) {
                return Some(rest.to_string());
            }
        }

        // Any host — match /{bucket}/{key_prefix}/… (internal k8s URLs, etc.)
        let marker = format!("/{}/{}/", self.bucket, self.key_prefix);
        if let Some(rest) = url.split(&marker).nth(1) {
            let suffix = rest.split(['?', '#']).next()?.trim_start_matches('/');
            if !suffix.is_empty() {
                return Some(format!("{}/{}", self.key_prefix, suffix));
            }
        }

        let virtual_host = format!("https://{}.s3.", self.bucket);
        if url.starts_with(&virtual_host) {
            let marker = ".amazonaws.com/";
            if let Some(rest) = url.split(marker).nth(1) {
                return Some(rest.to_string());
            }
        }

        None
    }

    /// Read an uploaded image by its stored URL or object key (S3 only — no arbitrary HTTP).
    pub async fn read_stored_image(&self, reference: &str) -> anyhow::Result<Vec<u8>> {
        let trimmed = reference.trim();
        if trimmed.is_empty() {
            anyhow::bail!("empty image reference");
        }

        if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            if let Some(key) = self.resolve_object_key(trimmed) {
                return self.get_object_bytes(&key).await;
            }
            anyhow::bail!("unsupported image url");
        }

        if trimmed.contains('/') {
            return self.get_object_bytes(trimmed).await;
        }

        decode_base64_bytes(trimmed)
    }

    fn resolve_object_key(&self, reference: &str) -> Option<String> {
        if let Some(key) = self.key_from_url(reference) {
            return Some(key);
        }

        let path = reference.split(['?', '#']).next()?;
        let marker = format!("/{}/", self.key_prefix);
        if let Some(rest) = path.split(&marker).nth(1) {
            let suffix = rest.trim_start_matches('/');
            if !suffix.is_empty() {
                return Some(format!("{}/{}", self.key_prefix, suffix));
            }
        }

        None
    }

    async fn get_object_bytes(&self, key: &str) -> anyhow::Result<Vec<u8>> {
        let response = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|err| anyhow::anyhow!("failed to read s3 object: {err}"))?;

        let bytes = response
            .body
            .collect()
            .await
            .map_err(|err| anyhow::anyhow!("failed to read s3 object body: {err}"))?
            .into_bytes()
            .to_vec();

        Ok(bytes)
    }
}

fn normalize_prefix(prefix: &str) -> String {
    prefix.trim().trim_matches('/').to_string()
}

fn default_public_base_url(cfg: &S3Config) -> String {
    if let Some(endpoint) = &cfg.endpoint {
        return format!(
            "{}/{}",
            endpoint.trim_end_matches('/'),
            cfg.bucket
        );
    }

    format!(
        "https://{}.s3.{}.amazonaws.com",
        cfg.bucket, cfg.region
    )
}

fn extension_for_content_type(content_type: &str) -> &'static str {
    match content_type {
        "image/png" => "png",
        "image/webp" => "webp",
        "image/gif" => "gif",
        _ => "jpg",
    }
}

async fn fetch_http_bytes(url: &str) -> anyhow::Result<Vec<u8>> {
    let response = reqwest::get(url)
        .await
        .map_err(|err| anyhow::anyhow!("failed to fetch image url: {err}"))?;

    if !response.status().is_success() {
        anyhow::bail!("failed to fetch image url: {}", response.status());
    }

    Ok(response
        .bytes()
        .await
        .map_err(|err| anyhow::anyhow!("failed to read fetched image: {err}"))?
        .to_vec())
}

fn decode_base64_bytes(value: &str) -> anyhow::Result<Vec<u8>> {
    use base64::{prelude::BASE64_STANDARD, Engine};

    let payload = if let Some(idx) = value.find(',') {
        value[idx + 1..].trim()
    } else {
        value.trim()
    };

    BASE64_STANDARD
        .decode(payload)
        .map_err(|_| anyhow::anyhow!("invalid base64 image data"))
}
