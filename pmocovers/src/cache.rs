use std::path::PathBuf;
use std::sync::Arc;
use anyhow::{anyhow, Result};
use sha1::{Sha1, Digest};
use tokio::sync::Mutex;
use crate::db::DB;
use crate::webp;

#[derive(Debug)]
pub struct Cache {
    pub(crate) dir: PathBuf,
    pub(crate) limit: usize,
    pub db: DB,
    mu: Arc<Mutex<()>>,
}

impl Cache {
    pub fn new(dir: &str, limit: usize) -> Result<Self> {
        std::fs::create_dir_all(dir)?;
        let db = DB::init(&PathBuf::from(dir).join("cache.db"))?;

        Ok(Self {
            dir: PathBuf::from(dir),
            limit,
            db,
            mu: Arc::new(Mutex::new(())),
        })
    }

    pub async fn add_from_url(&self, url: &str) -> Result<String> {
        let response = reqwest::get(url).await?;
        if !response.status().is_success() {
            return Err(anyhow!("Bad status: {}", response.status()));
        }

        let data = response.bytes().await?;
        self.add(url, &data).await
    }

    pub async fn ensure_from_url(&self, url: &str) -> Result<String> {
        let pk = pk_from_url(url);

        if self.db.get(&pk).is_ok() {
            let orig_path = self.dir.join(format!("{}.orig.webp", pk));
            if orig_path.exists() {
                return Ok(pk);
            }
        }

        self.add_from_url(url).await
    }

    pub async fn add(&self, url: &str, data: &[u8]) -> Result<String> {
        let pk = pk_from_url(url);
        let orig_path = self.dir.join(format!("{}.orig.webp", pk));

        let _lock = self.mu.lock().await;

        if !orig_path.exists() {
            let img = image::load_from_memory(data)?;
            let webp_data = webp::encode_webp(&img)?;
            tokio::fs::write(&orig_path, webp_data).await?;
        }

        self.db.add(&pk, url)?;
        Ok(pk)
    }

    pub async fn get(&self, pk: &str) -> Result<PathBuf> {
        let _lock = self.mu.lock().await;

        self.db.get(pk)?;
        self.db.update_hit(pk)?;

        let orig_path = self.dir.join(format!("{}.orig.webp", pk));
        if orig_path.exists() {
            Ok(orig_path)
        } else {
            Err(anyhow!("File not found"))
        }
    }

    pub async fn purge(&self) -> Result<()> {
        let _lock = self.mu.lock().await;

        let mut entries = tokio::fs::read_dir(&self.dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            if entry.path().is_file() {
                tokio::fs::remove_file(entry.path()).await?;
            }
        }

        self.db.purge().map_err(|e| anyhow!("Database error: {}", e))
    }

    pub async fn consolidate(&self) -> Result<()> {
        let _lock = self.mu.lock().await;

        let entries = self.db.get_all()?;

        for entry in entries {
            let orig_path = self.dir.join(format!("{}.orig.webp", entry.pk));
            if !orig_path.exists() {
                match reqwest::get(&entry.source_url).await {
                    Ok(response) if response.status().is_success() => {
                        let data = response.bytes().await?;
                        self.add(&entry.source_url, &data).await?;
                    }
                    _ => {
                        self.db.delete(&entry.pk)?;
                    }
                }
            }
        }

        let mut dir_entries = tokio::fs::read_dir(&self.dir).await?;
        while let Some(entry) = dir_entries.next_entry().await? {
            let path = entry.path();
            if path.is_file() {
                if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                    if file_name.ends_with(".orig.webp") {
                        let pk = file_name.trim_end_matches(".orig.webp");
                        if self.db.get(pk).is_err() {
                            tokio::fs::remove_file(path).await?;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    pub fn cache_dir(&self) -> String {
        self.dir.to_string_lossy().to_string()
    }
    
}

fn pk_from_url(url: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(url.as_bytes());
    let result = hasher.finalize();
    hex::encode(&result[..8])
}