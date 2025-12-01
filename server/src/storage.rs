use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use anyhow::{anyhow, Context};
use aws_sdk_s3::config::endpoint::DefaultResolver;
use aws_sdk_s3::config::Credentials;
use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart};
use aws_sdk_s3::{
    config::AsyncSleep, config::Region, config::SharedAsyncSleep, config::Sleep,
    primitives::ByteStream, Client,
};
use http::Uri;
use tokio::fs;
use tokio::io::AsyncReadExt;
use tokio::runtime::Runtime;
use tracing;

pub type ProgressCallback = Arc<dyn Fn(f32) + Send + Sync + 'static>;

const MULTIPART_MIN_SIZE: u64 = 5 * 1024 * 1024;
const MULTIPART_CHUNK_SIZE: usize = 8 * 1024 * 1024;
#[derive(Debug)]
pub struct ForeverSleep;

impl AsyncSleep for ForeverSleep {
    fn sleep(&self, _duration: std::time::Duration) -> Sleep {
        Sleep::new(std::future::pending())
    }
}
/// Simple content type guessing for a handful of formats we serve.
fn guess_content_type(path: &Path) -> &'static str {
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
            media_base_url,
        })
    }

    pub fn s3(
        media_root: PathBuf,
        bucket: String,
        region: String,
        access_key: String,
        secret_key: String,
        endpoint: String,
        prefix: String,
        public_base_url: String,
    ) -> anyhow::Result<Self> {
        let media_base_url = format!(
            "{}/{}",
            public_base_url.trim_end_matches('/'),
            prefix.trim_end_matches('/')
        );

        let rt = Runtime::new().context("create tokio runtime for S3 client")?;
        let client = {
            let region = Region::new(region);
            let credentials = Credentials::new(access_key, secret_key, None, None, "");
            let _endpoint_uri = match Uri::from_str(&endpoint) {
                Ok(u) => u,
                Err(e) => return Err(anyhow!("failed to create uri: {}", e)),
            };

            let resolver = DefaultResolver::new();
            let cfg = rt.block_on(async {
                let base = aws_config::defaults(aws_config::BehaviorVersion::latest())
                    .region(region)
                    .endpoint_url(endpoint)
                    .credentials_provider(credentials)
                    .load()
                    .await;
                aws_sdk_s3::config::Builder::from(&base)
                    .endpoint_resolver(resolver)
                    .sleep_impl(SharedAsyncSleep::new(ForeverSleep))
                    .build()
            });
            Client::from_conf(cfg)
        };

        Ok(StorageBackend::S3(ObjectStorageBackend {
            media_root,
            prefix,
            media_base_url,
            client: Arc::new(client),
            bucket,
            rt: Arc::new(rt),
        }))
    }

    pub fn media_base_url(&self) -> &str {
        match self {
            StorageBackend::LocalFs(b) => &b.media_base_url,
            StorageBackend::S3(b) => &b.media_base_url,
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

    fn key_for_path(&self, abs_path: &Path) -> anyhow::Result<String> {
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
    pub media_base_url: String,
}

#[derive(Clone)]
pub struct ObjectStorageBackend {
    pub media_root: PathBuf,
    pub prefix: String,
    pub media_base_url: String,
    pub bucket: String,
    pub client: Arc<Client>,
    pub rt: Arc<Runtime>,
}

impl ObjectStorageBackend {
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

        self.rt.block_on(async move {
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
