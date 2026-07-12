use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context};
use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart};
use aws_sdk_s3::{primitives::ByteStream, Client};
use tokio::fs;
use tokio::io::AsyncReadExt;
use tokio::runtime::Handle;
use tracing;

pub type ProgressCallback = Arc<dyn Fn(f32) + Send + Sync + 'static>;

const MULTIPART_MIN_SIZE: u64 = 5 * 1024 * 1024;
const MULTIPART_CHUNK_SIZE: usize = 8 * 1024 * 1024;
const S3_RETRY_COUNT: usize = 3;
const S3_RETRY_BASE_DELAY_MS: u64 = 250;

/// Build an S3 client with optional custom endpoint and region.
async fn build_s3_client(endpoint: &Option<String>, s3_region: Option<&str>) -> Client {
    let mut config_loader = aws_config::defaults(aws_config::BehaviorVersion::latest());

    // Only override endpoint for non-AWS S3 (MinIO, etc.)
    if let Some(ref ep) = endpoint {
        config_loader = config_loader.endpoint_url(ep);
    }
    if let Some(region) = s3_region {
        config_loader = config_loader.region(aws_sdk_s3::config::Region::new(region.to_string()));
    }

    let sdk_config = config_loader.load().await;
    let s3_config = aws_sdk_s3::config::Builder::from(&sdk_config)
        // Force path-style for MinIO compatibility
        .force_path_style(endpoint.is_some())
        .build();
    Client::from_conf(s3_config)
}

/// Simple retry helper for async S3 operations with exponential backoff.
async fn retry_s3_operation<F, Fut, T, E>(
    desc: &str,
    operation: F,
) -> anyhow::Result<T>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, aws_sdk_s3::error::SdkError<E>>>,
    E: std::fmt::Debug + std::fmt::Display + std::error::Error + Send + Sync + 'static,
{
    let mut last_err = None;
    for attempt in 0..S3_RETRY_COUNT {
        match operation().await {
            Ok(v) => return Ok(v),
            Err(e) => {
                tracing::warn!(attempt = attempt + 1, max = S3_RETRY_COUNT, err = %e, "S3 operation failed: {}", desc);
                last_err = Some(e);
                if attempt + 1 < S3_RETRY_COUNT {
                    let delay = S3_RETRY_BASE_DELAY_MS * 2u64.pow(attempt as u32);
                    tokio::time::sleep(Duration::from_millis(delay.min(5000))).await;
                }
            }
        }
    }
    Err(anyhow!("{} failed after {} attempts: {}", desc, S3_RETRY_COUNT, last_err.unwrap()))
}

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
    /// For AWS S3, leave endpoint as None and configure AWS_REGION, or pass s3_region.
    pub fn s3(
        media_root: PathBuf,
        bucket: String,
        endpoint: Option<String>,
        s3_region: Option<String>,
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

        // Build the SDK client. This function is called from a synchronous main(), so we need
        // a temporary runtime unless we're already inside one (tests may call us from a runtime).
        let client = match Handle::try_current() {
            Ok(handle) => {
                handle.block_on(async {
                    build_s3_client(&endpoint, s3_region.as_deref()).await
                })
            }
            Err(_) => {
                let rt = tokio::runtime::Runtime::new()
                    .context("create tokio runtime for S3 client init")?;
                rt.block_on(async {
                    build_s3_client(&endpoint, s3_region.as_deref()).await
                })
            }
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
    ///
    /// This is a synchronous wrapper that can be called from any thread. If a Tokio
    /// runtime handle is available it is reused; otherwise a temporary runtime is created.
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

    /// Async variant of [`Self::upload_with_progress`].
    ///
    /// Callers running inside a Tokio runtime should prefer this and wrap it in
    /// [`tokio::task::spawn_blocking`] if they need a synchronous interface.
    pub async fn upload_with_progress_async(
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
            StorageBackend::S3(backend) => backend.upload_with_progress_async(abs_path, progress).await,
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
        let fut = self.upload_with_progress_async(abs_path, progress);
        match Handle::try_current() {
            Ok(handle) => handle.block_on(fut),
            Err(_) => {
                let rt = tokio::runtime::Runtime::new()
                    .context("create tokio runtime for S3 upload")?;
                rt.block_on(fut)
            }
        }
    }

    async fn upload_with_progress_async(
        &self,
        abs_path: &Path,
        progress: Option<ProgressCallback>,
    ) -> anyhow::Result<()> {
        let key = StorageBackend::S3(self.clone()).key_for_path(abs_path)?;
        let ct = guess_content_type(abs_path);
        let bucket = self.bucket.clone();
        let client = self.client.clone();
        let path = abs_path.to_path_buf();

        let mut file = fs::File::open(&path)
            .await
            .with_context(|| format!("Open file {:?}", path))?;
        let meta = file.metadata().await?;
        let total_len = meta.len();

        if total_len == 0 {
            if let Some(cb) = progress.as_ref() {
                cb(1.0);
            }
            retry_s3_operation("put_object (empty)", || {
                let bucket = bucket.clone();
                let key = key.clone();
                let client = client.clone();
                async move {
                    client
                        .put_object()
                        .bucket(&bucket)
                        .key(&key)
                        .body(ByteStream::from(Vec::new()))
                        .content_type(ct)
                        .send()
                        .await
                }
            })
            .await
            .context("upload empty object to storage")?;
            return Ok(());
        }

        if total_len <= MULTIPART_MIN_SIZE {
            let mut buffer = Vec::with_capacity(total_len as usize);
            file.read_to_end(&mut buffer).await?;

            retry_s3_operation("put_object (small)", || {
                let bucket = bucket.clone();
                let key = key.clone();
                let client = client.clone();
                let body = ByteStream::from(buffer.clone());
                async move {
                    client
                        .put_object()
                        .bucket(&bucket)
                        .key(&key)
                        .body(body)
                        .content_type(ct)
                        .send()
                        .await
                }
            })
            .await
            .context("upload small object to storage")?;

            if let Some(cb) = progress {
                cb(1.0);
            }
            return Ok(());
        }

        let upload = retry_s3_operation("create_multipart_upload", || {
            let bucket = bucket.clone();
            let key = key.clone();
            let client = client.clone();
            async move {
                client
                    .create_multipart_upload()
                    .bucket(&bucket)
                    .key(&key)
                    .content_type(ct)
                    .send()
                    .await
            }
        })
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

            let pn = part_number;
            let res = retry_s3_operation(&format!("upload_part {part_number}"),
                || {
                    let bucket = bucket.clone();
                    let key = key.clone();
                    let upload_id = upload_id.clone();
                    let client = client.clone();
                    let body = ByteStream::from(buf[..chunk_size].to_vec());
                    async move {
                        client
                            .upload_part()
                            .bucket(&bucket)
                            .key(&key)
                            .upload_id(&upload_id)
                            .part_number(pn)
                            .body(body)
                            .send()
                            .await
                    }
                },
            )
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

        if let Err(e) = retry_s3_operation("complete_multipart_upload", || {
            let bucket = bucket.clone();
            let key = key.clone();
            let upload_id = upload_id.clone();
            let client = client.clone();
            let multipart = multipart.clone();
            async move {
                client
                    .complete_multipart_upload()
                    .bucket(&bucket)
                    .key(&key)
                    .upload_id(&upload_id)
                    .multipart_upload(multipart)
                    .send()
                    .await
            }
        })
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
                .upload_id(&upload_id)
                .send()
                .await;
            return Err(anyhow!("complete multipart upload: {e}"));
        }

        if let Some(cb) = progress {
            cb(1.0);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn test_guess_content_type() {
        assert_eq!(guess_content_type(Path::new("foo.mp4")), "video/mp4");
        assert_eq!(guess_content_type(Path::new("foo.MP4")), "video/mp4");
        assert_eq!(guess_content_type(Path::new("foo.mkv")), "video/x-matroska");
        assert_eq!(guess_content_type(Path::new("foo.webm")), "video/webm");
        assert_eq!(guess_content_type(Path::new("foo.mov")), "video/quicktime");
        assert_eq!(guess_content_type(Path::new("foo.webp")), "image/webp");
        assert_eq!(guess_content_type(Path::new("foo.png")), "image/png");
        assert_eq!(guess_content_type(Path::new("foo.jpg")), "image/jpeg");
        assert_eq!(guess_content_type(Path::new("foo.jpeg")), "image/jpeg");
        assert_eq!(guess_content_type(Path::new("foo.vtt")), "text/vtt");
        assert_eq!(guess_content_type(Path::new("foo.srt")), "application/x-subrip");
        assert_eq!(guess_content_type(Path::new("foo.unknown")), "application/octet-stream");
        assert_eq!(guess_content_type(Path::new("foo")), "application/octet-stream");
    }

    #[test]
    fn test_local_backend_accessors() {
        let storage = StorageBackend::local(PathBuf::from("/data/videos"), "http://localhost:8080");
        assert_eq!(storage.media_base_url(), "http://localhost:8080/videos");
        assert_eq!(storage.media_root(), Path::new("/data/videos"));
        assert!(!storage.needs_remote_upload());
    }

    #[test]
    fn test_local_media_url() {
        let storage = StorageBackend::local(PathBuf::from("/data/videos"), "http://localhost:8080/");
        assert_eq!(
            storage.media_url("abc123/video.mp4"),
            "http://localhost:8080/videos/abc123/video.mp4"
        );
        assert_eq!(
            storage.media_url("/abc123/thumbs/thumb.webp"),
            "http://localhost:8080/videos/abc123/thumbs/thumb.webp"
        );
    }

    #[test]
    fn test_key_for_path_local() {
        let storage = StorageBackend::local(PathBuf::from("/data/videos"), "http://localhost:8080");
        assert_eq!(
            storage.key_for_path(Path::new("/data/videos/abc123/video.mp4")).unwrap(),
            "videos/abc123/video.mp4"
        );
        assert!(storage.key_for_path(Path::new("/outside/video.mp4")).is_err());
    }

    #[test]
    fn test_key_for_path_normalizes_backslashes() {
        // Even on Unix this documents the intended cross-platform behaviour.
        let storage = StorageBackend::local(PathBuf::from("/data/videos"), "http://localhost:8080");
        let key = storage.key_for_path(Path::new("/data/videos/abc123\\video.mp4")).unwrap();
        assert_eq!(key, "videos/abc123/video.mp4");
    }

    #[test]
    fn test_presigned_url_local_errors() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = StorageBackend::local(PathBuf::from("/data/videos"), "http://localhost:8080");
        let res = rt.block_on(storage.presigned_url("abc123", "video.mp4"));
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("presigned URLs are not available"));
    }

    #[test]
    fn test_upload_with_progress_local_invokes_callback() {
        let storage = StorageBackend::local(PathBuf::from("/data/videos"), "http://localhost:8080");
        let called = Arc::new(AtomicBool::new(false));
        let called2 = called.clone();
        let cb: ProgressCallback = Arc::new(move |p| {
            assert!((p - 1.0).abs() < f32::EPSILON);
            called2.store(true, Ordering::SeqCst);
        });
        storage.upload_with_progress(Path::new("/data/videos/foo.mp4"), Some(cb)).unwrap();
        assert!(called.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_upload_with_progress_async_local_invokes_callback() {
        let storage = StorageBackend::local(PathBuf::from("/data/videos"), "http://localhost:8080");
        let called = Arc::new(AtomicBool::new(false));
        let called2 = called.clone();
        let cb: ProgressCallback = Arc::new(move |p| {
            assert!((p - 1.0).abs() < f32::EPSILON);
            called2.store(true, Ordering::SeqCst);
        });
        storage.upload_with_progress_async(Path::new("/data/videos/foo.mp4"), Some(cb)).await.unwrap();
        assert!(called.load(Ordering::SeqCst));
    }

    #[test]
    fn test_upload_with_progress_local_from_blocking_thread() {
        // The sync wrapper should reuse the current runtime handle from a blocking
        // thread (spawn_blocking), which is how async callers should invoke it.
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = StorageBackend::local(PathBuf::from("/data/videos"), "http://localhost:8080");
        let called = Arc::new(AtomicBool::new(false));
        let called2 = called.clone();
        let cb: ProgressCallback = Arc::new(move |p| {
            assert!((p - 1.0).abs() < f32::EPSILON);
            called2.store(true, Ordering::SeqCst);
        });
        rt.block_on(async {
            tokio::task::spawn_blocking(move || {
                storage.upload_with_progress(Path::new("/data/videos/foo.mp4"), Some(cb)).unwrap();
            })
            .await
            .unwrap();
        });
        assert!(called.load(Ordering::SeqCst));
    }

    #[test]
    fn test_upload_if_exists_local_noop_for_missing_path() {
        let storage = StorageBackend::local(PathBuf::from("/data/videos"), "http://localhost:8080");
        storage.upload_if_exists(Path::new("/data/videos/does-not-exist.mp4"));
    }

    #[test]
    fn test_upload_required_local_noop() {
        let storage = StorageBackend::local(PathBuf::from("/data/videos"), "http://localhost:8080");
        storage.upload_required(Path::new("/data/videos/foo.mp4")).unwrap();
    }

    #[test]
    fn test_s3_default_public_base_url_for_aws() {
        let storage = StorageBackend::s3(
            PathBuf::from("/data/videos"),
            "my-bucket".to_string(),
            None,
            Some("eu-west-1".to_string()),
            "media".to_string(),
            None,
            "http://localhost:8080".to_string(),
            Duration::from_secs(3600),
        )
        .expect("failed to create AWS S3 backend");
        assert_eq!(storage.media_base_url(), "https://my-bucket.s3.amazonaws.com/media");
    }

    #[test]
    fn test_s3_default_public_base_url_for_minio() {
        let storage = StorageBackend::s3(
            PathBuf::from("/data/videos"),
            "my-bucket".to_string(),
            Some("http://minio.example.com:9000".to_string()),
            None,
            "media".to_string(),
            None,
            "http://localhost:8080".to_string(),
            Duration::from_secs(3600),
        )
        .expect("failed to create MinIO backend");
        assert_eq!(storage.media_base_url(), "http://minio.example.com:9000/my-bucket/media");
    }

    #[test]
    fn test_s3_media_url_and_needs_remote_upload() {
        let storage = StorageBackend::s3(
            PathBuf::from("/data/videos"),
            "my-bucket".to_string(),
            Some("http://minio.example.com".to_string()),
            None,
            "videos".to_string(),
            None,
            "http://localhost:8080".to_string(),
            Duration::from_secs(3600),
        )
        .expect("failed to create S3 backend");
        assert!(storage.needs_remote_upload());
        assert_eq!(
            storage.media_url("abc123/video.mp4"),
            "http://localhost:8080/api/media/abc123/video.mp4"
        );
    }

    #[test]
    fn test_s3_key_for_path_with_prefix() {
        let storage = StorageBackend::s3(
            PathBuf::from("/data/videos"),
            "my-bucket".to_string(),
            Some("http://minio.example.com".to_string()),
            None,
            "uploads".to_string(),
            None,
            "http://localhost:8080".to_string(),
            Duration::from_secs(3600),
        )
        .expect("failed to create S3 backend");
        assert_eq!(
            storage.key_for_path(Path::new("/data/videos/abc123/video.mp4")).unwrap(),
            "uploads/abc123/video.mp4"
        );
    }

    #[test]
    fn test_s3_key_for_path_without_prefix() {
        let storage = StorageBackend::s3(
            PathBuf::from("/data/videos"),
            "my-bucket".to_string(),
            Some("http://minio.example.com".to_string()),
            None,
            "".to_string(),
            None,
            "http://localhost:8080".to_string(),
            Duration::from_secs(3600),
        )
        .expect("failed to create S3 backend");
        assert_eq!(
            storage.key_for_path(Path::new("/data/videos/abc123/video.mp4")).unwrap(),
            "abc123/video.mp4"
        );
    }
}
