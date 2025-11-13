use anyhow::Result;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::{CacheConfig, DB};

/// Trait générique pour les caches de fichiers
///
/// Définit l'interface commune pour tous les types de caches (images, audio, etc.)
pub trait FileCache<C: CacheConfig>: Send + Sync {
    fn get_cache_dir(&self) -> &Path;
    fn get_database(&self) -> Arc<DB>;

    /// Valide les données avant de les stocker dans le cache
    ///
    /// Cette méthode peut être surchargée pour vérifier le type MIME,
    /// le magic number, ou effectuer des conversions (ex: WebP, FLAC)
    ///
    /// # Arguments
    ///
    /// * `data` - Données brutes à valider
    ///
    /// # Returns
    ///
    /// Les données validées/converties ou une erreur
    fn validate_data(&self, data: &[u8]) -> Result<Vec<u8>> {
        // Par défaut, on accepte les données telles quelles
        Ok(data.to_vec())
    }

    /// Retourne le type de cache
    fn cache_type(&self) -> &'static str {
        C::cache_type()
    }

    /// Retourne le nom du cache
    fn cache_name(&self) -> &'static str {
        C::cache_name()
    }

    /// Retourne le paramètre par défaut
    fn default_param(&self) -> &'static str {
        C::default_param()
    }

    /// Retourne l'extension des fichiers
    fn file_extension(&self) -> &'static str {
        C::file_extension()
    }

    /// Construit le chemin complet d'un fichier dans le cache
    ///
    /// Format: `{pk}.{qualificatif}.{extension}`
    /// Pour le fichier original: `{pk}.orig.{extension}`
    fn file_path(&self, pk: &str) -> PathBuf {
        self.file_path_with_qualifier(pk, self.default_param())
    }

    /// Construit le chemin d'un fichier avec un qualificatif
    ///
    /// Format: `{pk}.{qualificatif}.{extension}`
    fn file_path_with_qualifier(&self, pk: &str, qualifier: &str) -> PathBuf {
        self.get_cache_dir()
            .join(format!("{}.{}.{}", pk, qualifier, C::file_extension()))
    }

    /// Retourne la route relative pour accéder à un item du cache
    ///
    /// # Arguments
    ///
    /// * `pk` - Clé primaire de la piste
    /// * `param` - Paramètre optionnel (ex: "orig", "128k", etc.)
    ///
    /// # Returns
    ///
    /// Route relative (ex: "/audio/flac/abc123" ou "/audio/tracks/abc123/orig")
    fn route_for(&self, pk: &str, param: Option<&str>) -> String {
        if let Some(p) = param {
            format!("/{}/{}/{}/{}", C::cache_name(), C::cache_type(), pk, p)
        } else {
            format!("/{}/{}/{}", C::cache_name(), C::cache_type(), pk)
        }
    }

    /// Télécharge un fichier depuis une URL et l'ajoute au cache
    ///
    /// # Arguments
    ///
    /// * `url` - URL du fichier à télécharger
    /// * `collection` - Collection optionnelle à laquelle appartient le fichier
    ///
    /// # Returns
    ///
    /// La clé primaire (pk) du fichier dans le cache
    async fn add_from_url(&self, url: &str, collection: Option<&str>) -> Result<String>;

    /// Ajoute un fichier local au cache
    ///
    /// Le fichier est copié dans le cache via une URL file://
    ///
    /// # Arguments
    ///
    /// * `path` - Chemin du fichier local
    /// * `collection` - Collection optionnelle à laquelle appartient le fichier
    ///
    /// # Returns
    ///
    /// La clé primaire (pk) du fichier dans le cache
    async fn add_from_file(&self, path: &str, collection: Option<&str>) -> Result<String>;

    /// Récupère le chemin d'un fichier dans le cache
    ///
    /// # Arguments
    ///
    /// * `pk` - Clé primaire du fichier
    async fn get(&self, pk: &str) -> Result<PathBuf>;

    /// Récupère tous les fichiers d'une collection
    ///
    /// # Arguments
    ///
    /// * `collection` - Identifiant de la collection
    async fn get_collection(&self, collection: &str) -> Result<Vec<PathBuf>>;

    /// Supprime tous les fichiers et entrées du cache
    async fn purge(&self) -> Result<()>;

    /// Consolide le cache en supprimant les orphelins et en re-téléchargeant les fichiers manquants
    async fn consolidate(&self) -> Result<()>;

    /// Vérifie si une clé primaire est valide (existe en DB et fichier présent)
    ///
    /// # Arguments
    ///
    /// * `pk` - Clé primaire à vérifier
    ///
    /// # Returns
    ///
    /// `true` si l'entrée existe en base de données et que le fichier est présent ET:
    /// - SOIT le fichier est complet (marker .complete existe)
    /// - SOIT le download est en cours (fichier récent sans marker)
    ///
    /// Ceci permet le progressive caching: les fichiers en cours de download sont acceptés
    /// dès que le prebuffer est atteint, sans attendre le marker de completion.
    async fn is_valid_pk(&self, pk: &str) -> bool {
        if self.get_database().get(pk, false).is_err() {
            tracing::debug!("is_valid_pk({}): DB entry not found", pk);
            return false;
        }

        let file_path = self.file_path(pk);
        if !file_path.exists() {
            // Si l'entrée DB existe mais pas le fichier, c'est probablement en cours d'ingestion
            // Attendre jusqu'à 1 seconde que le fichier soit créé (le tokio::spawn peut mettre un peu de temps)
            tracing::debug!("is_valid_pk({}): File does not exist yet, waiting for file creation (ingestion in progress)", pk);

            let mut attempts = 0;
            while !file_path.exists() && attempts < 100 {
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                attempts += 1;
            }

            if !file_path.exists() {
                tracing::warn!("is_valid_pk({}): File not created after 1s despite DB entry existing", pk);
                return false;
            }

            tracing::debug!("is_valid_pk({}): File created after {}ms", pk, attempts * 10);
        }

        // Vérifier d'abord si le marker de completion existe
        let completion_marker = file_path.with_extension(
            format!("{}.complete", C::file_extension())
        );

        if completion_marker.exists() {
            tracing::debug!("is_valid_pk({}): Completion marker found, file is complete", pk);
            return true;
        }

        // Pas de marker - vérifier si le download est en cours (fichier récent)
        // Un fichier en cours de download aura une modification récente
        if let Ok(metadata) = file_path.metadata() {
            if let Ok(modified) = metadata.modified() {
                if let Ok(elapsed) = modified.elapsed() {
                    let age_secs = elapsed.as_secs();
                    if age_secs < 60 {
                        tracing::debug!("is_valid_pk({}): No marker but file is recent ({}s), download in progress", pk, age_secs);
                        return true;
                    } else {
                        tracing::debug!("is_valid_pk({}): No marker and file is old ({}s), incomplete download", pk, age_secs);
                        return false;
                    }
                }
            }
        }

        // Ne peut pas vérifier le statut - rejeter par sécurité
        tracing::debug!("is_valid_pk({}): Could not check file status, rejecting", pk);
        false
    }
}

/// Génère une clé primaire à partir des premiers octets d'un document
///
/// Utilise SHA256 pour hasher les premiers octets du contenu et retourne les 16 premiers octets
/// en hexadécimal (32 caractères). L'utilisation de 16 octets au lieu de 8 réduit considérablement
/// les risques de collision.
///
/// # Arguments
///
/// * `header` - Les premiers octets du document (typiquement 512 octets)
///
/// # Returns
///
/// Une chaîne hexadécimale de 32 caractères servant de clé primaire unique
///
/// # Exemple
///
/// ```
/// use pmocache::pk_from_content_header;
///
/// let data = b"Some file content...";
/// let pk = pk_from_content_header(data);
/// assert_eq!(pk.len(), 32);  // 16 bytes = 32 hex chars
/// ```
pub fn pk_from_content_header(header: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(header);
    let result = hasher.finalize();
    hex::encode(&result[..16]) // 16 octets = 32 caractères hex
}
