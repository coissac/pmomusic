use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

/// Retourne le préfixe d'un lazy PK (`PREFIX:VALUE`)
pub fn lazy_prefix_from_pk(lazy_pk: &str) -> Option<&str> {
    lazy_pk.split_once(':').map(|(prefix, _)| prefix)
}

/// Données optionnelles pouvant être fournies par un [`LazyProvider`]
#[derive(Debug, Clone, Default)]
pub struct LazyEntryRemoteData {
    pub metadata: Option<Value>,
    pub cover_url: Option<String>,
}

/// Trait générique décrivant un fournisseur de lazy PK.
///
/// Chaque implémentation est responsable d'un préfixe particulier (ex: `QOBUZ`).
/// Lorsque le cache rencontre un lazy PK dont le préfixe correspond,
/// il délègue au provider pour résoudre l'URL et récupérer les informations
/// nécessaires (métadonnées, couverture, etc.).
#[async_trait]
pub trait LazyProvider: Send + Sync {
    /// Préfixe associé (sans le `:` final).
    fn lazy_prefix(&self) -> &'static str;

    /// Retourne l'URL de téléchargement actuelle pour ce lazy PK.
    async fn get_url(&self, lazy_pk: &str) -> Result<String>;

    /// Métadonnées optionnelles à associer immédiatement à l'entrée lazy.
    async fn metadata(&self, lazy_pk: &str) -> Result<Option<Value>> {
        let _ = lazy_pk;
        Ok(None)
    }

    /// URL de couverture éventuelle pour permettre un cache eager des jaquettes.
    async fn cover_url(&self, lazy_pk: &str) -> Result<Option<String>> {
        let _ = lazy_pk;
        Ok(None)
    }
}
