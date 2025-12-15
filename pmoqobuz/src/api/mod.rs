//! Couche d'accès à l'API REST Qobuz
//!
//! Ce module fournit une interface bas-niveau pour communiquer avec l'API Qobuz.

pub mod auth;
pub mod catalog;
pub mod signing;
pub mod spoofer;
pub mod user;

use crate::error::{QobuzError, Result};
use crate::models::AudioFormat;
use reqwest::{Client, Response};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::sync::RwLock;
use std::time::Duration;
use tracing::{debug, warn};

pub use spoofer::Spoofer;

/// URL de base de l'API Qobuz
const API_BASE_URL: &str = "https://www.qobuz.com/api.json/0.2";
//const API_BASE_URL: &str = "http://localhost:8080/api.json/0.2";

/// App ID Qobuz par défaut
///
/// Cet App ID est un fallback au cas où :
/// - Aucun appID n'est configuré dans pmoconfig
/// - Le Spoofer n'est pas disponible ou échoue
///
/// Note: Cet App ID peut devenir obsolète avec le temps.
/// Il est recommandé d'utiliser soit la configuration manuelle,
/// soit le Spoofer pour obtenir un App ID à jour.
pub const DEFAULT_APP_ID: &str = "1401488693436528";

/// Client API bas-niveau pour communiquer avec Qobuz
pub struct QobuzApi {
    /// Client HTTP
    client: Client,
    /// App ID pour l'authentification
    app_id: RwLock<String>,
    /// Secret s4 pour signer les requêtes sensibles (track/getFileUrl, userLibrary/*)
    ///
    /// Ce secret est obtenu soit :
    /// - En décodant un `configvalue` (base64) et XOR avec l'app_id
    /// - Depuis le Spoofer (secrets dynamiques)
    secret: RwLock<Option<Vec<u8>>>,
    /// Token d'authentification utilisateur
    user_auth_token: RwLock<Option<String>>,
    /// ID utilisateur
    user_id: RwLock<Option<String>>,
    /// Format audio par défaut
    format_id: AudioFormat,
}

impl QobuzApi {
    /// Crée une nouvelle instance de l'API
    pub fn new(app_id: impl Into<String>) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:67.0) Gecko/20100101 Firefox/67.0",
            )
            .build()?;

        Ok(Self {
            client,
            app_id: RwLock::new(app_id.into()),
            secret: RwLock::new(None),
            user_auth_token: RwLock::new(None),
            user_id: RwLock::new(None),
            format_id: AudioFormat::default(),
        })
    }

    /// Crée une API avec un secret depuis configvalue (base64)
    ///
    /// # Arguments
    ///
    /// * `app_id` - App ID Qobuz
    /// * `configvalue` - Secret encodé en base64 (à XORer avec l'app_id)
    ///
    /// # Note
    ///
    /// Cette méthode reproduit le comportement Python de `__set_s4()`.
    /// Le configvalue est décodé depuis base64, puis XORé avec l'app_id
    /// pour obtenir le secret s4.
    pub fn with_secret(app_id: impl Into<String>, configvalue: &str) -> Result<Self> {
        let api = Self::new(app_id)?;
        api.set_secret_from_configvalue(configvalue)?;
        Ok(api)
    }

    /// Crée une API avec un secret brut (déjà décodé/dérivé)
    ///
    /// # Arguments
    ///
    /// * `app_id` - App ID Qobuz
    /// * `raw_secret` - Secret prêt à l'emploi (comme ceux du Spoofer)
    ///
    /// # Note
    ///
    /// Utilisez cette méthode pour les secrets du Spoofer qui sont déjà
    /// décodés et prêts à l'emploi (pas besoin de XOR avec l'app_id)
    pub fn with_raw_secret(app_id: impl Into<String>, raw_secret: &str) -> Result<Self> {
        let api = Self::new(app_id)?;
        api.set_secret(raw_secret.as_bytes().to_vec());
        Ok(api)
    }

    /// Définit le secret s4 directement
    ///
    /// # Arguments
    ///
    /// * `secret` - Secret s4 en bytes (déjà décodé et dérivé)
    pub fn set_secret(&self, secret: Vec<u8>) {
        *self.secret.write().unwrap() = Some(secret);
    }

    /// Dérive et définit le secret s4 depuis un configvalue
    ///
    /// Reproduit la logique Python de `__set_s4()`:
    /// 1. Décode le configvalue depuis base64
    /// 2. XOR avec l'app_id
    /// 3. Stocke le résultat comme secret s4
    fn set_secret_from_configvalue(&self, configvalue: &str) -> Result<()> {
        use base64::{engine::general_purpose::STANDARD, Engine};

        // Décoder le configvalue depuis base64
        let s3s = STANDARD
            .decode(configvalue.trim())
            .map_err(|e| QobuzError::Configuration(format!("Invalid configvalue: {}", e)))?;

        // XOR avec l'app_id
        let app_id = self.app_id.read().unwrap();
        let app_id_bytes = app_id.as_bytes();
        let mut s4 = Vec::with_capacity(s3s.len());

        for (i, &byte) in s3s.iter().enumerate() {
            let app_byte = app_id_bytes[i % app_id_bytes.len()];
            s4.push(byte ^ app_byte);
        }

        *self.secret.write().unwrap() = Some(s4);
        Ok(())
    }

    /// Retourne le secret s4 si disponible
    pub fn secret(&self) -> Option<Vec<u8>> {
        self.secret.read().unwrap().clone()
    }

    /// Définit le token d'authentification
    pub fn set_auth_token(&self, token: String, user_id: String) {
        *self.user_auth_token.write().unwrap() = Some(token);
        *self.user_id.write().unwrap() = Some(user_id);
    }

    /// Efface les informations d'authentification
    pub fn clear_auth(&self) {
        *self.user_auth_token.write().unwrap() = None;
        *self.user_id.write().unwrap() = None;
    }

    /// Définit le format audio par défaut
    pub fn set_format(&mut self, format: AudioFormat) {
        self.format_id = format;
    }

    /// Retourne le format audio configuré
    pub fn format(&self) -> AudioFormat {
        self.format_id
    }

    /// Retourne l'App ID
    pub fn app_id(&self) -> String {
        self.app_id.read().unwrap().clone()
    }

    /// Met à jour dynamiquement l'app_id et le secret associés (avec XOR)
    pub fn update_credentials(&self, app_id: impl Into<String>, configvalue: &str) -> Result<()> {
        {
            let mut current = self.app_id.write().unwrap();
            *current = app_id.into();
        }

        self.set_secret_from_configvalue(configvalue)
    }

    /// Met à jour dynamiquement l'app_id et le secret brut (sans XOR)
    pub fn update_credentials_raw(&self, app_id: impl Into<String>, raw_secret: &str) {
        {
            let mut current = self.app_id.write().unwrap();
            *current = app_id.into();
        }

        self.set_secret(raw_secret.as_bytes().to_vec());
    }

    /// Retourne le token d'authentification si disponible
    pub fn auth_token(&self) -> Option<String> {
        self.user_auth_token.read().unwrap().clone()
    }

    /// Retourne l'ID utilisateur si disponible
    pub fn user_id(&self) -> Option<String> {
        self.user_id.read().unwrap().clone()
    }

    /// Effectue une requête GET à l'API
    pub(crate) async fn get<T: DeserializeOwned>(
        &self,
        endpoint: &str,
        params: &[(&str, &str)],
    ) -> Result<T> {
        self.request("GET", endpoint, params).await
    }

    /// Effectue une requête POST à l'API
    pub(crate) async fn post<T: DeserializeOwned>(
        &self,
        endpoint: &str,
        params: &[(&str, &str)],
    ) -> Result<T> {
        self.request("POST", endpoint, params).await
    }

    /// Effectue une requête à l'API (générique)
    async fn request<T: DeserializeOwned>(
        &self,
        method: &str,
        endpoint: &str,
        params: &[(&str, &str)],
    ) -> Result<T> {
        let url = format!("{}{}", API_BASE_URL, endpoint);

        debug!("{} {} with {} params", method, url, params.len());

        let mut request = if method == "GET" {
            self.client.get(&url)
        } else {
            self.client.post(&url)
        };

        // Ajouter les headers
        let app_id = self.app_id.read().unwrap().clone();
        request = request.header("X-App-Id", app_id);

        if let Some(token) = self.auth_token() {
            request = request.header("X-User-Auth-Token", token);
        }

        // Headers additionnels pour compatibilité avec qobuz-player-client
        request = request.header(
            "Accept-Language",
            "en,en-US;q=0.8,ko;q=0.6,zh;q=0.4,zh-CN;q=0.2",
        );
        request = request.header(
            "Access-Control-Request-Headers",
            "x-user-auth-token,x-app-id",
        );

        // Ajouter les paramètres
        if method == "GET" {
            request = request.query(params);
        } else {
            request = request.form(params);
        }

        // Envoyer la requête
        let response = request.send().await?;
        self.handle_response(response, endpoint).await
    }

    /// Traite la réponse HTTP
    async fn handle_response<T: DeserializeOwned>(
        &self,
        response: Response,
        endpoint: &str,
    ) -> Result<T> {
        let status = response.status();
        let status_code = status.as_u16();

        debug!("Response status: {}", status);

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            debug!(
                "API error ({}) on {}: {}",
                status_code, endpoint, error_text
            );
            return Err(QobuzError::from_status_code(status_code, error_text));
        }

        let text = response.text().await?;

        // Vérifier si la réponse contient une erreur Qobuz
        if let Ok(json) = serde_json::from_str::<Value>(&text) {
            if let Some(status_obj) = json.get("status") {
                if status_obj == "error" {
                    let message = json
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("Unknown error");
                    debug!("Qobuz API error on {}: {}", endpoint, message);
                    return Err(QobuzError::ApiError {
                        code: status_code,
                        message: message.to_string(),
                    });
                }
            }
        }

        // Parser la réponse
        serde_json::from_str(&text).map_err(|e| {
            debug!("Failed to parse response: {}", e);
            QobuzError::JsonParse(e)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_creation() {
        let api = QobuzApi::new("test_app_id").unwrap();
        assert_eq!(api.app_id(), "test_app_id".to_string());
        assert!(api.auth_token().is_none());
    }

    #[test]
    fn test_set_auth_token() {
        let api = QobuzApi::new("test_app_id").unwrap();
        api.set_auth_token("test_token".to_string(), "user123".to_string());
        assert_eq!(api.auth_token().as_deref(), Some("test_token"));
        assert_eq!(api.user_id().as_deref(), Some("user123"));
    }

    #[test]
    fn test_set_format() {
        let mut api = QobuzApi::new("test_app_id").unwrap();
        api.set_format(AudioFormat::Flac_HiRes_96);
        assert_eq!(api.format(), AudioFormat::Flac_HiRes_96);
    }
}
