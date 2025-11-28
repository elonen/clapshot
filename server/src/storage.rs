use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use anyhow::{anyhow, bail, Context};
use aws_sdk_s3::{config::Region, config::endpoint::Endpoint, primitives::ByteStream, Client, config::endpoint::ResolveEndpoint};
use aws_sdk_s3::config::auth::{ParamsBuilder};
use aws_sdk_s3::config::Credentials;
use aws_sdk_s3::config::endpoint::{DefaultResolver, EndpointFuture, SharedEndpointResolver};
use http::Uri;
use mime::Params;
use tokio::runtime::Runtime;
/// Simple content type guessing for a handful of formats we serve.
fn guess_content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase()) {
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
        let media_base_url = format!("{}/{}", public_base_url.trim_end_matches('/'), prefix.trim_end_matches('/'));

        let rt = Runtime::new().context("create tokio runtime for S3 client")?;
        let client = {
            let region = Region::new(region);
            let credentials = Credentials::new(access_key, secret_key, None, None, "");
            let url=match Uri::from_str(&endpoint){
                Ok(u) => u,
                Err(e) => return Err(anyhow!("failed to create uri: {}", e)),
            };

            let resolver=DefaultResolver::new();
            let cfg = rt.block_on(async {
                let base = aws_config::defaults(aws_config::BehaviorVersion::latest())
                    .region(region)
                    .endpoint_url(endpoint)
                    .credentials_provider(credentials)
                    .load()
                    .await;
                aws_sdk_s3::config::Builder::from(&base)
                    .endpoint_resolver(resolver)
                    .force_path_style(true)
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
        match self {
            StorageBackend::LocalFs(_) => Ok(()),
            StorageBackend::S3(backend) => backend.upload(abs_path),
        }
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

    fn key_for_path(&self, abs_path: &Path) -> anyhow::Result<String> {
        let root = self.media_root();
        let rel = abs_path.strip_prefix(root)
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
    fn upload(&self, abs_path: &Path) -> anyhow::Result<()> {
        let key = StorageBackend::S3(self.clone()).key_for_path(abs_path)?;
        let ct = guess_content_type(abs_path);
        let mut file = File::open(abs_path).with_context(|| format!("Open file {:?}", abs_path))?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;

        self.rt.block_on(async {
            let stream = ByteStream::from(buffer);
            self.client
                .put_object()
                .bucket(&self.bucket)
                .key(&key)
                .body(stream)
                .content_type(ct)
                .send()
                .await
        })
        .context("upload to object storage")?;

        Ok(())
    }
}
