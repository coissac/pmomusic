use anyhow::Result;
use sha1::{Digest, Sha1};
use std::{path::{Path, PathBuf}, sync::Arc};

use crate::{CacheConfig, DB};

/// Trait générique pour les caches de fichiers
///
/// Définit l'interface commune pour tous les types de caches (images, audio, etc.)
pub trait FileCache<C: CacheConfig>: Send + Sync {

    fn get_cache_dir(&self) -> &Path;
    fn get_database(&self) ->  Arc<DB>;
    fn get_base_url(&self) -> &str;

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

    /// Retourne le nom de la table
    fn table_name(&self) -> &'static str {
        C::table_name()
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
        self.get_cache_dir().join(format!("{}.{}.{}", pk, qualifier, C::file_extension()))
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

    /// S'assure qu'un fichier est présent dans le cache
    ///
    /// Si le fichier existe déjà, retourne sa clé. Sinon, le télécharge.
    ///
    /// # Arguments
    ///
    /// * `url` - URL du fichier
    /// * `collection` - Collection optionnelle à laquelle appartient le fichier
    async fn ensure_from_url(&self, url: &str, collection: Option<&str>) -> Result<String>;

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

}

/// Génère une clé primaire à partir d'une URL
///
/// Utilise SHA1 pour hasher l'URL et retourne les 8 premiers octets en hexadécimal.
pub fn pk_from_url(url: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(url.as_bytes());
    let result = hasher.finalize();
    hex::encode(&result[..8])
}
