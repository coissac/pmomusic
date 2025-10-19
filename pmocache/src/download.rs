use futures_util::Future;
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// Type pour une fonction de transformation de stream
///
/// La fonction reçoit:
/// - Le stream de bytes téléchargés
/// - Un writer pour écrire les données transformées
/// - Un callback pour mettre à jour la progression
///
/// Elle retourne un Future qui se résout en Result
pub type StreamTransformer = Box<
    dyn FnOnce(
            reqwest::Response,
            tokio::fs::File,
            Arc<dyn Fn(u64) + Send + Sync>,
        ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>>
        + Send,
>;

/// État interne du téléchargement
#[derive(Debug, Clone)]
struct DownloadState {
    /// Taille actuelle téléchargée (du stream source)
    current_size: u64,
    /// Taille attendue du fichier source (si connue)
    expected_size: Option<u64>,
    /// Taille des données transformées écrites
    transformed_size: u64,
    /// Indique si le téléchargement est terminé
    finished: bool,
    /// Position de lecture actuelle
    read_position: u64,
    /// Erreur éventuelle lors du téléchargement
    error: Option<String>,
}

/// Objet représentant un téléchargement en cours
#[derive(Debug)]
pub struct Download {
    /// Nom du fichier de destination
    filename: PathBuf,
    /// État partagé entre le téléchargement et les lectures
    state: Arc<RwLock<DownloadState>>,
}

impl Download {
    /// Crée une nouvelle instance de Download
    fn new(filename: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            filename,
            state: Arc::new(RwLock::new(DownloadState {
                current_size: 0,
                expected_size: None,
                transformed_size: 0,
                finished: false,
                read_position: 0,
                error: None,
            })),
        })
    }

    /// Retourne le nom du fichier
    pub fn filename(&self) -> &Path {
        &self.filename
    }

    /// Attend que le fichier atteigne au moins la taille spécifiée ou soit complètement téléchargé
    pub async fn wait_until_min_size(&self, min_size: u64) -> Result<(), String> {
        loop {
            let state = self.state.read().await;

            // Vérifier s'il y a eu une erreur
            if let Some(ref error) = state.error {
                return Err(error.clone());
            }

            // Vérifier si la condition est remplie
            if state.transformed_size >= min_size || state.finished {
                return Ok(());
            }

            drop(state); // Libérer le lock avant de dormir
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// Attend que le téléchargement soit complètement terminé
    pub async fn wait_until_finished(&self) -> Result<(), String> {
        loop {
            let state = self.state.read().await;

            // Vérifier s'il y a eu une erreur
            if let Some(ref error) = state.error {
                return Err(error.clone());
            }

            if state.finished {
                return Ok(());
            }

            drop(state);
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// Ouvre le fichier pour lecture
    pub fn open(&self) -> io::Result<File> {
        File::open(&self.filename)
    }

    /// Retourne la position actuelle de lecture
    pub async fn pos(&self) -> u64 {
        let state = self.state.read().await;
        state.read_position
    }

    /// Met à jour la position de lecture
    pub async fn set_pos(&self, pos: u64) {
        let mut state = self.state.write().await;
        state.read_position = pos;
    }

    /// Retourne la taille attendue du fichier (si disponible)
    pub async fn expected_size(&self) -> Option<u64> {
        let state = self.state.read().await;
        state.expected_size
    }

    /// Retourne la taille actuellement téléchargée (du stream source)
    pub async fn current_size(&self) -> u64 {
        let state = self.state.read().await;
        state.current_size
    }

    /// Retourne la taille des données transformées écrites sur disque
    pub async fn transformed_size(&self) -> u64 {
        let state = self.state.read().await;
        state.transformed_size
    }

    /// Indique si le téléchargement est terminé
    pub async fn finished(&self) -> bool {
        let state = self.state.read().await;
        state.finished
    }

    /// Retourne l'erreur éventuelle
    pub async fn error(&self) -> Option<String> {
        let state = self.state.read().await;
        state.error.clone()
    }
}

/// Lance le téléchargement d'une URL dans un fichier
///
/// # Arguments
/// * `filename` - Chemin du fichier de destination
/// * `url` - URL à télécharger
///
/// # Returns
/// Un Arc<Download> qui permet de suivre la progression du téléchargement
pub fn download<P: AsRef<Path>>(filename: P, url: &str) -> Arc<Download> {
    download_with_transformer(filename, url, None)
}

/// Lance le téléchargement d'une URL avec transformation du stream
///
/// # Arguments
/// * `filename` - Chemin du fichier de destination
/// * `url` - URL à télécharger
/// * `transformer` - Fonction optionnelle pour transformer le stream avant sauvegarde
///
/// # Returns
/// Un Arc<Download> qui permet de suivre la progression du téléchargement
///
/// # Exemple
///
/// ```rust,no_run
/// use pmocache::download::{download_with_transformer, StreamTransformer};
/// use futures_util::StreamExt;
/// use tokio::io::AsyncWriteExt;
///
/// // Transformer qui convertit en majuscules (exemple simple)
/// let transformer: StreamTransformer = Box::new(|response, mut file, update_progress| {
///     Box::pin(async move {
///         let mut stream = response.bytes_stream();
///         let mut total = 0u64;
///
///         while let Some(chunk_result) = stream.next().await {
///             let chunk = chunk_result.map_err(|e| e.to_string())?;
///
///             // Transformer les données (ex: conversion, décompression, etc.)
///             let transformed = chunk.to_vec(); // Votre transformation ici
///
///             file.write_all(&transformed).await.map_err(|e| e.to_string())?;
///
///             total += chunk.len() as u64;
///             update_progress(total);
///         }
///
///         file.flush().await.map_err(|e| e.to_string())?;
///         Ok(())
///     })
/// });
///
/// let dl = download_with_transformer("/tmp/output.txt", "https://example.com/data", Some(transformer));
/// ```
pub fn download_with_transformer<P: AsRef<Path>>(
    filename: P,
    url: &str,
    transformer: Option<StreamTransformer>,
) -> Arc<Download> {
    let filename = filename.as_ref().to_path_buf();
    let url = url.to_string();

    let download = Download::new(filename.clone());
    let state = Arc::clone(&download.state);

    // Lancer le téléchargement en tâche de fond
    tokio::spawn(async move {
        if let Err(e) = download_impl(filename, url, state, transformer).await {
            // L'erreur a déjà été enregistrée dans download_impl
            eprintln!("Download error: {}", e);
        }
    });

    download
}

/// Implémentation du téléchargement
async fn download_impl(
    filename: PathBuf,
    url: String,
    state: Arc<RwLock<DownloadState>>,
    transformer: Option<StreamTransformer>,
) -> Result<(), String> {
    // Créer le client HTTP
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|e| e.to_string())?;

    // Lancer la requête
    let response = client.get(&url).send().await.map_err(|e| {
        let error = format!("Failed to fetch URL: {}", e);
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let mut s = state.write().await;
                s.error = Some(error.clone());
            });
        });
        error
    })?;

    // Vérifier le statut
    if !response.status().is_success() {
        let error = format!("HTTP error: {}", response.status());
        let mut s = state.write().await;
        s.error = Some(error.clone());
        s.finished = true;
        return Err(error);
    }

    // Récupérer la taille attendue si disponible
    if let Some(content_length) = response.content_length() {
        let mut s = state.write().await;
        s.expected_size = Some(content_length);
    }

    // Créer le fichier de destination
    let file = tokio::fs::File::create(&filename).await.map_err(|e| {
        let error = format!("Failed to create file: {}", e);
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let mut s = state.write().await;
                s.error = Some(error.clone());
                s.finished = true;
            });
        });
        error
    })?;

    // Si un transformer est fourni, l'utiliser
    if let Some(transformer) = transformer {
        // Créer un callback pour mettre à jour la progression
        let state_clone = Arc::clone(&state);
        let progress_callback: Arc<dyn Fn(u64) + Send + Sync> =
            Arc::new(move |transformed_bytes| {
                let state = Arc::clone(&state_clone);
                tokio::spawn(async move {
                    let mut s = state.write().await;
                    s.transformed_size = transformed_bytes;
                });
            });

        // Appeler le transformer
        match transformer(response, file, progress_callback).await {
            Ok(_) => {
                let mut s = state.write().await;
                s.finished = true;
                Ok(())
            }
            Err(e) => {
                let mut s = state.write().await;
                s.error = Some(e.clone());
                s.finished = true;
                Err(e)
            }
        }
    } else {
        // Comportement par défaut : téléchargement direct sans transformation
        default_download(response, file, state).await
    }
}

/// Téléchargement par défaut sans transformation
async fn default_download(
    response: reqwest::Response,
    mut file: tokio::fs::File,
    state: Arc<RwLock<DownloadState>>,
) -> Result<(), String> {
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    let mut stream = response.bytes_stream();

    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(chunk) => {
                // Écrire le chunk dans le fichier
                if let Err(e) = file.write_all(&chunk).await {
                    let error = format!("Failed to write to file: {}", e);
                    let mut s = state.write().await;
                    s.error = Some(error.clone());
                    s.finished = true;
                    return Err(error);
                }

                // Mettre à jour les tailles (identiques sans transformation)
                let mut s = state.write().await;
                let chunk_len = chunk.len() as u64;
                s.current_size += chunk_len;
                s.transformed_size += chunk_len;
            }
            Err(e) => {
                let error = format!("Failed to read chunk: {}", e);
                let mut s = state.write().await;
                s.error = Some(error.clone());
                s.finished = true;
                return Err(error);
            }
        }
    }

    // Fermer le fichier
    if let Err(e) = file.flush().await {
        let error = format!("Failed to flush file: {}", e);
        let mut s = state.write().await;
        s.error = Some(error.clone());
        s.finished = true;
        return Err(error);
    }

    // Marquer comme terminé
    let mut s = state.write().await;
    s.finished = true;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_download_basic() {
        let temp_dir = std::env::temp_dir();
        let filename = temp_dir.join("test_download.txt");

        // Nettoyer si le fichier existe
        let _ = std::fs::remove_file(&filename);

        // Télécharger un petit fichier de test
        let dl = download(&filename, "https://www.rust-lang.org/");

        // Attendre la fin du téléchargement
        match dl.wait_until_finished().await {
            Ok(_) => {
                assert!(dl.finished().await);
                assert!(filename.exists());
                assert!(dl.current_size().await > 0);
            }
            Err(e) => {
                eprintln!("Download failed: {}", e);
            }
        }

        // Nettoyer
        let _ = std::fs::remove_file(&filename);
    }

    #[tokio::test]
    async fn test_wait_until_min_size() {
        let temp_dir = std::env::temp_dir();
        let filename = temp_dir.join("test_download_min_size.txt");

        let _ = std::fs::remove_file(&filename);

        let dl = download(&filename, "https://www.rust-lang.org/");

        // Attendre au moins 100 bytes
        match dl.wait_until_min_size(100).await {
            Ok(_) => {
                let size = dl.current_size().await;
                assert!(size >= 100 || dl.finished().await);
            }
            Err(e) => {
                eprintln!("Download failed: {}", e);
            }
        }

        let _ = std::fs::remove_file(&filename);
    }
}
