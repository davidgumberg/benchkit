use anyhow::{Context, Result};
use object_store::aws::AmazonS3;
use object_store::{path::Path, ObjectStore};
use std::fs;
use std::path::PathBuf;
use tokio::io::AsyncReadExt;

pub struct ObjectStorage {
    store: AmazonS3,
    bucket: String,
}

impl ObjectStorage {
    pub fn new(store: AmazonS3, bucket: String) -> Self {
        Self { store, bucket }
    }

    pub fn from_env() -> Result<Self> {
        let key_id =
            std::env::var("KEY_ID").with_context(|| "KEY_ID environment variable not set")?;
        let secret_key = std::env::var("SECRET_ACCESS_KEY")
            .with_context(|| "SECRET_ACCESS_KEY environment variable not set")?;
        let endpoint = std::env::var("OBJECT_STORAGE_URL")
            .unwrap_or_else(|_| "https://hel1.your-objectstorage.com".to_string());
        let bucket =
            std::env::var("OBJECT_STORAGE_BUCKET").unwrap_or_else(|_| "benchcoin".to_string());

        let store = object_store::aws::AmazonS3Builder::new()
            .with_bucket_name(&bucket)
            .with_access_key_id(key_id)
            .with_secret_access_key(secret_key)
            .with_endpoint(&endpoint)
            .build()?;

        Ok(Self::new(store, bucket))
    }

    pub async fn upload_file(&self, local_path: &PathBuf, remote_path: &str) -> Result<()> {
        let content = fs::read(local_path)
            .with_context(|| format!("Failed to read file: {}", local_path.display()))?;

        self.store
            .put(&Path::from(remote_path), content.into())
            .await
            .with_context(|| format!("Failed to upload file to {}", remote_path))?;

        Ok(())
    }

    pub async fn download_file(&self, remote_path: &str, local_path: &PathBuf) -> Result<()> {
        let mut data = self
            .store
            .get(&Path::from(remote_path))
            .await
            .with_context(|| format!("Failed to get file from {}", remote_path))?;

        let mut buffer = Vec::new();
        data.read_to_end(&mut buffer)
            .await
            .with_context(|| format!("Failed to read data from {}", remote_path))?;

        fs::write(local_path, buffer)
            .with_context(|| format!("Failed to write file to {}", local_path.display()))?;

        Ok(())
    }

    pub async fn list_files(&self, prefix: Option<&str>) -> Result<Vec<String>> {
        let mut list_stream = self.store.list(prefix.map(Path::from));
        let mut files = Vec::new();

        while let Some(meta) = list_stream.next().await {
            match meta {
                Ok(meta) => {
                    files.push(meta.location.to_string());
                }
                Err(e) => eprintln!("Error listing object: {}", e),
            }
        }

        Ok(files)
    }

    pub async fn upload_benchmark_run(
        &self,
        run_id: i32,
        benchmark_name: &str,
        run_number: usize,
        config_path: &PathBuf,
        benchmark_path: &PathBuf,
        data_dir: &PathBuf,
        results_path: Option<&PathBuf>,
    ) -> Result<()> {
        // Create base directory for the run
        let base_path = format!("{}", run_id);
        let benchmark_dir = format!("{}/{}", base_path, benchmark_name.replace(" ", "_"));
        let run_dir = format!("{}/{}", benchmark_dir, run_number);

        // Upload config files if they exist and this is the first run
        if run_number == 0 {
            if config_path.exists() {
                self.upload_file(config_path, &format!("{}/config.yml", base_path))
                    .await?;
            }
            if benchmark_path.exists() {
                self.upload_file(benchmark_path, &format!("{}/benchmark.yml", base_path))
                    .await?;
            }
        }

        // Upload debug.log if it exists
        let debug_log = data_dir.join("debug.log");
        if debug_log.exists() {
            self.upload_file(&debug_log, &format!("{}/debug.log", run_dir))
                .await?;
        }

        // Upload results.json if provided and this is the final run
        if let Some(results) = results_path {
            if results.exists() {
                self.upload_file(results, &format!("{}/results.json", base_path))
                    .await?;
            }
        }

        Ok(())
    }
}
