use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context};
use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart};
use aws_sdk_s3::{primitives::ByteStream, Client};
use tokio::fs;
use tokio::io::AsyncReadExt;
use tracing;

pub type ProgressCallback = Arc<dyn Fn(f32) + Send + Sync + 'static>;

const MULTIPART_MIN_SIZE: u64 = 5 * 1024 * 1024;
const MULTIPART_CHUNK_SIZE: usize = 8 * 1024 * 1024;
/// Simple content type guessing for a handful of formats we serve.
pub(crate) fn guess_content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
    {
        Some(ext) if ext == "mp4" => "video/mp4",
        Some(ext) if ext == "mkv" => "video/x-matroska",
        Some(ext) if ext == "webm" => "video/webm",
        Some(ext) if ext == "mov" => "video/quicktime",
        Some(ext) if ext == "webp" => "image/webp",
        Some(ext) if ext == "png" => "image/png",
        Some(ext) if ext == "jpg" || ext == "jpeg" => "image/jpeg",
        Some(ext) if ext == "vtt" => "text/vtt",
        Some(ext) if ext == "srt" => "application/x-subrip",
        _ => "application/octet-stream",
    }
}

#[derive(Clone)]
pub enum StorageBackend {
    LocalFs(LocalFsBackend),
    S3(ObjectStorageBackend),
}

impl StorageBackend {
    pub fn local(media_root: PathBuf, url_base: &str) -> Self {
        let prefix = "videos".to_string();
        let media_base_url = format!("{}/{}", url_base.trim_end_matches('/'), prefix);
        StorageBackend::LocalFs(LocalFsBackend {
            media_root,
            prefix,
            url_base: url_base.to_string(),
            media_base_url,
        })
    }

    /// Create an S3 storage backend using the AWS SDK default credential chain.
    ///
    /// Credentials are resolved automatically in this order:
    /// 1. Environment variables: AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY
    /// 2. Shared credentials file: ~/.aws/credentials
    /// 3. AWS config file: ~/.aws/config (with profiles)
    /// 4. ECS container credentials
    /// 5. EC2 instance metadata (IAM role)
    ///
    /// For MinIO or other S3-compatible storage, set endpoint to the service URL.
    /// For AWS S3, leave endpoint as None and set AWS_REGION environment variable.
    pub fn s3(
        media_root: PathBuf,
        bucket: String,
        endpoint: Option<String>,
        prefix: String,
        public_base_url: Option<String>,
        url_base: String,
        presigned_url_expiry: Duration,
    ) -> anyhow::Result<Self> {
        let public_base_url = public_base_url.unwrap_or_else(|| {
            endpoint.as_ref().map(|ep| format!("{}/{}", ep.trim_end_matches('/'), &bucket))
                .unwrap_or_else(|| format!("https://{}.s3.amazonaws.com", &bucket))
        });
        let media_base_url = format!(
            "{}/{}",
            public_base_url.trim_end_matches('/'),
            prefix.trim_end_matches('/')
        );

        // Create a temporary runtime just for client initialization.
        // The client survives after the runtime is dropped.
        // We don't persist the runtime to avoid "cannot drop runtime in async context" panics
        // when the storage is dropped inside another tokio runtime (e.g., during server shutdown).
        let client = {
            let rt = tokio::runtime::Runtime::new().context("create tokio runtime for S3 client init")?;
            let client = rt.block_on(async {
                let mut config_loader = aws_config::defaults(aws_config::BehaviorVersion::latest());

                // Only override endpoint for non-AWS S3 (MinIO, etc.)
                if let Some(ref ep) = endpoint {
                    config_loader = config_loader.endpoint_url(ep);
                }

                let sdk_config = config_loader.load().await;
                let s3_config = aws_sdk_s3::config::Builder::from(&sdk_config)
                    // Force path-style for MinIO compatibility
                    .force_path_style(endpoint.is_some())
                    .build();
                Client::from_conf(s3_config)
            });
            // rt is dropped here, but client survives
            client
        };

        Ok(StorageBackend::S3(ObjectStorageBackend {
            media_root,
            prefix,
            url_base,
            media_base_url,
            client: Arc::new(client),
            bucket,
            presigned_url_expiry,
        }))
    }

    pub fn media_base_url(&self) -> &str {
        match self {
            StorageBackend::LocalFs(b) => &b.media_base_url,
            StorageBackend::S3(b) => &b.media_base_url,
        }
    }

    /// Return the client-visible URL for a media path.
    ///
    /// For the local backend this is a direct `/videos/...` URL.
    /// For S3 this is a `/api/media/...` URL that the server will redirect to a
    /// short-lived presigned S3 URL.
    pub fn media_url(&self, rel_path: &str) -> String {
        let rel_path = rel_path.trim_start_matches('/');
        match self {
            StorageBackend::LocalFs(b) => format!("{}/{}", b.media_base_url.trim_end_matches('/'), rel_path),
            StorageBackend::S3(b) => format!("{}/api/media/{}", b.url_base.trim_end_matches('/'), rel_path),
        }
    }

    /// Generate a short-lived presigned S3 URL for a media path.
    ///
    /// Returns an error for the local backend.
    pub async fn presigned_url(&self, media_id: &str, rel_path: &str) -> anyhow::Result<String> {
        match self {
            StorageBackend::LocalFs(_) => Err(anyhow!("presigned URLs are not available for local filesystem backend")),
            StorageBackend::S3(backend) => backend.presigned_url(media_id, rel_path).await,
        }
    }

    pub fn media_root(&self) -> &Path {
        match self {
            StorageBackend::LocalFs(b) => &b.media_root,
            StorageBackend::S3(b) => &b.media_root,
        }
    }

    pub fn needs_remote_upload(&self) -> bool {
        matches!(self, StorageBackend::S3(_))
    }

    /// Upload a file that lives under the media root. No-op for LocalFS.
    pub fn upload_local_path(&self, abs_path: &Path) -> anyhow::Result<()> {
        self.upload_with_progress(abs_path, None)
    }

    /// Upload file if it exists and log an error instead of bailing.
    pub fn upload_if_exists(&self, abs_path: &Path) {
        if !self.needs_remote_upload() {
            return;
        }
        if !abs_path.exists() {
            tracing::debug!(path=?abs_path, "Skipping upload for missing file");
            return;
        }
        if let Err(e) = self.upload_local_path(abs_path) {
            tracing::error!(path=?abs_path, details=%e, "Failed to upload asset to object storage");
        }
    }

    /// Upload a file when object storage is enabled, and propagate failures.
    pub fn upload_required(&self, abs_path: &Path) -> anyhow::Result<()> {
        if !self.needs_remote_upload() {
            return Ok(());
        }
        self.upload_with_progress(abs_path, None)
    }

    /// Upload a file, optionally reporting progress (0.0 - 1.0) while streaming to object storage.
    pub fn upload_with_progress(
        &self,
        abs_path: &Path,
        progress: Option<ProgressCallback>,
    ) -> anyhow::Result<()> {
        match self {
            StorageBackend::LocalFs(_) => {
                if let Some(cb) = progress {
                    cb(1.0);
                }
                Ok(())
            }
            StorageBackend::S3(backend) => backend.upload_with_progress(abs_path, progress),
        }
    }

    pub(crate) fn key_for_path(&self, abs_path: &Path) -> anyhow::Result<String> {
        let root = self.media_root();
        let rel = abs_path
            .strip_prefix(root)
            .with_context(|| format!("Path '{:?}' not under media root '{:?}'", abs_path, root))?;
        let rel = rel.to_string_lossy().replace('\\', "/");
        let prefix = match self {
            StorageBackend::LocalFs(b) => &b.prefix,
            StorageBackend::S3(b) => &b.prefix,
        }
        .trim_end_matches('/');

        if prefix.is_empty() {
            Ok(rel)
        } else {
            Ok(format!("{}/{}", prefix, rel))
        }
    }
}

#[derive(Clone)]
pub struct LocalFsBackend {
    pub media_root: PathBuf,
    pub prefix: String,
    pub url_base: String,
    pub media_base_url: String,
}

#[derive(Clone)]
pub struct ObjectStorageBackend {
    pub media_root: PathBuf,
    pub prefix: String,
    pub url_base: String,
    pub media_base_url: String,
    pub bucket: String,
    pub client: Arc<Client>,
    pub presigned_url_expiry: Duration,
}

impl ObjectStorageBackend {
    /// Generate a short-lived presigned S3 URL for `media_id/rel_path`.
    async fn presigned_url(&self,
        media_id: &str,
        rel_path: &str,
    ) -> anyhow::Result<String> {
        let rel_path = rel_path.trim_start_matches('/');
        let prefix = self.prefix.trim_end_matches('/');
        let key = if prefix.is_empty() {
            format!("{}/{}", media_id, rel_path)
        } else {
            format!("{}/{}/{}", prefix, media_id, rel_path)
        };

        let presign_config = PresigningConfig::expires_in(self.presigned_url_expiry)
            .context("invalid presigned URL expiry")?;

        let presigned = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(&key)
            .presigned(presign_config)
            .await
            .context("failed to presign S3 URL")?;

        Ok(presigned.uri().to_string())
    }

    fn upload_with_progress(
        &self,
        abs_path: &Path,
        progress: Option<ProgressCallback>,
    ) -> anyhow::Result<()> {
        let key = StorageBackend::S3(self.clone()).key_for_path(abs_path)?;
        let ct = guess_content_type(abs_path);
        let bucket = self.bucket.clone();
        let client = self.client.clone();
        let path = abs_path.to_path_buf();

        // Create a fresh runtime for each upload to avoid "cannot drop runtime in async context" panics.
        // This is slightly less efficient than reusing a runtime, but much safer when the storage
        // is held by code that runs inside another tokio runtime (like the api_server).
        let rt = tokio::runtime::Runtime::new().context("create tokio runtime for S3 upload")?;
        rt.block_on(async move {
            let mut file = fs::File::open(&path)
                .await
                .with_context(|| format!("Open file {:?}", path))?;
            let meta = file.metadata().await?;
            let total_len = meta.len();

            if total_len == 0 {
                if let Some(cb) = progress.as_ref() {
                    cb(1.0);
                }
                client
                    .put_object()
                    .bucket(&bucket)
                    .key(&key)
                    .body(ByteStream::from(Vec::new()))
                    .content_type(ct)
                    .send()
                    .await
                    .context("upload empty object to storage")?;
                return Ok(());
            }

            if total_len <= MULTIPART_MIN_SIZE {
                let mut buffer = Vec::with_capacity(total_len as usize);
                file.read_to_end(&mut buffer).await?;

                client
                    .put_object()
                    .bucket(&bucket)
                    .key(&key)
                    .body(ByteStream::from(buffer))
                    .content_type(ct)
                    .send()
                    .await
                    .context("upload small object to storage")?;

                if let Some(cb) = progress {
                    cb(1.0);
                }
                return Ok(());
            }

            let upload = client
                .create_multipart_upload()
                .bucket(&bucket)
                .key(&key)
                .content_type(ct)
                .send()
                .await
                .context("initiate multipart upload")?;

            let upload_id = upload
                .upload_id()
                .ok_or(anyhow!("Missing upload id from multipart upload"))?
                .to_string();

            let mut parts = Vec::new();
            let mut buf = vec![0u8; MULTIPART_CHUNK_SIZE];
            let mut part_number = 1;
            let mut uploaded: u64 = 0;

            loop {
                // Read a complete chunk (read() may return short reads with async I/O)
                // S3 requires all parts except the last to be >= 5MB
                let mut chunk_size = 0;
                loop {
                    let bytes_read = file.read(&mut buf[chunk_size..]).await?;
                    if bytes_read == 0 {
                        break; // EOF
                    }
                    chunk_size += bytes_read;
                    if chunk_size >= MULTIPART_CHUNK_SIZE {
                        break; // Full chunk
                    }
                }

                if chunk_size == 0 {
                    break; // No more data
                }

                let body = ByteStream::from(buf[..chunk_size].to_vec());
                let res = client
                    .upload_part()
                    .bucket(&bucket)
                    .key(&key)
                    .upload_id(&upload_id)
                    .part_number(part_number)
                    .body(body)
                    .send()
                    .await
                    .with_context(|| format!("upload part {part_number}"))?;

                let etag = res
                    .e_tag()
                    .ok_or(anyhow!("Missing etag for uploaded part {part_number}"))?
                    .to_string();

                parts.push(
                    CompletedPart::builder()
                        .e_tag(etag)
                        .part_number(part_number)
                        .build(),
                );

                uploaded += chunk_size as u64;
                if let Some(cb) = progress.as_ref() {
                    cb((uploaded as f32 / total_len as f32).clamp(0.0, 1.0));
                }

                part_number += 1;
            }

            let multipart = CompletedMultipartUpload::builder()
                .set_parts(Some(parts))
                .build();

            if let Err(e) = client
                .complete_multipart_upload()
                .bucket(&bucket)
                .key(&key)
                .upload_id(&upload_id)
                .multipart_upload(multipart)
                .send()
                .await
            {
                tracing::error!(
                    details=%e,
                    upload_id=%upload_id,
                    key=%key,
                    "Completing multipart upload failed, aborting"
                );
                // Best-effort abort; ignore abort error to bubble the original failure.
                let _ = client
                    .abort_multipart_upload()
                    .bucket(&bucket)
                    .key(&key)
                    .upload_id(upload_id)
                    .send()
                    .await;
                return Err(anyhow!("complete multipart upload: {e}"));
            }

            if let Some(cb) = progress {
                cb(1.0);
            }
            Ok::<(), anyhow::Error>(())
        })?;

        Ok(())
    }
}
