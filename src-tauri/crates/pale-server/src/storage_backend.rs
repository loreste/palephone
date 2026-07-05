//! Pluggable storage backend: local disk or S3-compatible object storage.
//!
//! When `PALE_S3_BUCKET` is set, files are stored in S3 (works with AWS S3,
//! MinIO, Nextcloud, or any S3-compatible endpoint).  Otherwise, files go to
//! the local `data_dir/files` directory as before.

use std::path::PathBuf;

use aws_sdk_s3::primitives::ByteStream;
use serde::{Deserialize, Serialize};

/// Describes which storage backend is active.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StorageBackend {
    Local {
        files_dir: PathBuf,
    },
    S3 {
        bucket: String,
        region: String,
        endpoint: Option<String>,
    },
}

/// Configuration parsed from environment variables.
#[derive(Debug, Clone)]
pub struct S3Config {
    pub bucket: String,
    pub region: String,
    pub endpoint: Option<String>,
    pub access_key: Option<String>,
    pub secret_key: Option<String>,
}

impl S3Config {
    /// Reads S3 configuration from environment variables.  Returns `None` when
    /// `PALE_S3_BUCKET` is not set, meaning the local backend should be used.
    pub fn from_env() -> Option<Self> {
        let bucket = std::env::var("PALE_S3_BUCKET")
            .ok()
            .filter(|v| !v.is_empty())?;
        let region = std::env::var("PALE_S3_REGION")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "us-east-1".to_string());
        let endpoint = std::env::var("PALE_S3_ENDPOINT")
            .ok()
            .filter(|v| !v.is_empty());
        let access_key = std::env::var("PALE_S3_ACCESS_KEY")
            .ok()
            .filter(|v| !v.is_empty());
        let secret_key = std::env::var("PALE_S3_SECRET_KEY")
            .ok()
            .filter(|v| !v.is_empty());
        Some(Self {
            bucket,
            region,
            endpoint,
            access_key,
            secret_key,
        })
    }
}

/// Holds an initialized S3 client (or local dir) ready for I/O.
#[derive(Clone)]
pub struct StorageClient {
    inner: StorageInner,
}

#[derive(Clone)]
enum StorageInner {
    Local {
        files_dir: PathBuf,
    },
    S3 {
        client: aws_sdk_s3::Client,
        bucket: String,
    },
}

impl std::fmt::Debug for StorageClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.inner {
            StorageInner::Local { files_dir } => f
                .debug_struct("StorageClient::Local")
                .field("files_dir", files_dir)
                .finish(),
            StorageInner::S3 { bucket, .. } => f
                .debug_struct("StorageClient::S3")
                .field("bucket", bucket)
                .finish(),
        }
    }
}

impl StorageClient {
    /// Create a local-only storage client.
    pub fn local(files_dir: PathBuf) -> Self {
        Self {
            inner: StorageInner::Local { files_dir },
        }
    }

    /// Create an S3-backed storage client from the given configuration.
    pub async fn s3(config: &S3Config) -> Self {
        let mut aws_builder = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(aws_config::Region::new(config.region.clone()));

        if let (Some(access_key), Some(secret_key)) = (&config.access_key, &config.secret_key) {
            aws_builder = aws_builder.credentials_provider(aws_sdk_s3::config::Credentials::new(
                access_key.clone(),
                secret_key.clone(),
                None,
                None,
                "pale-env",
            ));
        }

        let aws_config = aws_builder.load().await;

        let mut s3_config_builder =
            aws_sdk_s3::config::Builder::from(&aws_config).force_path_style(true);

        if let Some(endpoint) = &config.endpoint {
            s3_config_builder = s3_config_builder.endpoint_url(endpoint);
        }

        let client = aws_sdk_s3::Client::from_conf(s3_config_builder.build());
        Self {
            inner: StorageInner::S3 {
                client,
                bucket: config.bucket.clone(),
            },
        }
    }

    /// Upload data to the given object path.  Returns the path/key used.
    pub async fn upload(&self, path: &str, data: &[u8]) -> Result<String, StorageError> {
        match &self.inner {
            StorageInner::Local { files_dir } => {
                let full_path = files_dir.join(path);
                if let Some(parent) = full_path.parent() {
                    tokio::fs::create_dir_all(parent).await?;
                }
                tokio::fs::write(&full_path, data).await?;
                Ok(path.to_string())
            }
            StorageInner::S3 { client, bucket } => {
                client
                    .put_object()
                    .bucket(bucket)
                    .key(path)
                    .body(ByteStream::from(data.to_vec()))
                    .send()
                    .await
                    .map_err(|e| StorageError::S3(format!("put_object failed: {e}")))?;
                Ok(path.to_string())
            }
        }
    }

    /// Download data from the given object path.
    pub async fn download(&self, path: &str) -> Result<Vec<u8>, StorageError> {
        match &self.inner {
            StorageInner::Local { files_dir } => {
                let full_path = files_dir.join(path);
                let data = tokio::fs::read(&full_path).await?;
                Ok(data)
            }
            StorageInner::S3 { client, bucket } => {
                let resp = client
                    .get_object()
                    .bucket(bucket)
                    .key(path)
                    .send()
                    .await
                    .map_err(|e| StorageError::S3(format!("get_object failed: {e}")))?;
                let bytes = resp
                    .body
                    .collect()
                    .await
                    .map_err(|e| StorageError::S3(format!("body collect failed: {e}")))?;
                Ok(bytes.to_vec())
            }
        }
    }

    /// Delete the object at the given path.
    pub async fn delete(&self, path: &str) -> Result<(), StorageError> {
        match &self.inner {
            StorageInner::Local { files_dir } => {
                let full_path = files_dir.join(path);
                match tokio::fs::remove_file(&full_path).await {
                    Ok(()) => Ok(()),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                    Err(e) => Err(e.into()),
                }
            }
            StorageInner::S3 { client, bucket } => {
                client
                    .delete_object()
                    .bucket(bucket)
                    .key(path)
                    .send()
                    .await
                    .map_err(|e| StorageError::S3(format!("delete_object failed: {e}")))?;
                Ok(())
            }
        }
    }

    /// Check connectivity to the storage backend.
    pub async fn check_connectivity(&self) -> Result<(), StorageError> {
        match &self.inner {
            StorageInner::Local { files_dir } => {
                tokio::fs::create_dir_all(files_dir).await?;
                Ok(())
            }
            StorageInner::S3 { client, bucket } => {
                client
                    .head_bucket()
                    .bucket(bucket)
                    .send()
                    .await
                    .map_err(|e| StorageError::S3(format!("head_bucket failed: {e}")))?;
                Ok(())
            }
        }
    }

    /// Return a descriptor of which backend is active.
    pub fn backend_info(&self) -> StorageBackend {
        match &self.inner {
            StorageInner::Local { files_dir } => StorageBackend::Local {
                files_dir: files_dir.clone(),
            },
            StorageInner::S3 { bucket, .. } => {
                let endpoint = std::env::var("PALE_S3_ENDPOINT").ok();
                let region =
                    std::env::var("PALE_S3_REGION").unwrap_or_else(|_| "us-east-1".to_string());
                StorageBackend::S3 {
                    bucket: bucket.clone(),
                    region,
                    endpoint,
                }
            }
        }
    }

    /// Returns true if this is using S3.
    pub fn is_s3(&self) -> bool {
        matches!(self.inner, StorageInner::S3 { .. })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("s3: {0}")]
    S3(String),
}
