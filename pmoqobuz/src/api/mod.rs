//! Couche d'accès à l'API REST Qobuz
//!
//! Ce module fournit une interface bas-niveau pour communiquer avec l'API Qobuz.

pub mod auth;
pub mod catalog;
pub mod user;

use crate::error::{QobuzError, Result};
use crate::models::AudioFormat;
use reqwest::{Client, Response};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::time::Duration;
use tracing::{debug, warn};

/// URL de base de l'API Qobuz
const API_BASE_URL: &str = "https://www.qobuz.com/api.json/0.2";

/// Client API bas-niveau pour communiquer avec Qobuz
pub struct QobuzApi {
    /// Client HTTP
    client: Client,
    /// App ID pour l'authentification
    app_id: String,
    /// Token d'authentification utilisateur
    user_auth_token: Option<String>,
    /// ID utilisateur
    user_id: Option<String>,
    /// Format audio par défaut
    format_id: AudioFormat,
}

impl QobuzApi {
    /// Crée une nouvelle instance de l'API
    pub fn new(app_id: impl Into<String>) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:67.0) Gecko/20100101 Firefox/67.0")
            .build()?;

        Ok(Self {
            client,
            app_id: app_id.into(),
            user_auth_token: None,
            user_id: None,
            format_id: AudioFormat::default(),
        })
    }

    /// Définit le token d'authentification
    pub fn set_auth_token(&mut self, token: String, user_id: String) {
        self.user_auth_token = Some(token);
        self.user_id = Some(user_id);
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
    pub fn app_id(&self) -> &str {
        &self.app_id
    }

    /// Retourne le token d'authentification si disponible
    pub fn auth_token(&self) -> Option<&str> {
        self.user_auth_token.as_deref()
    }

    /// Retourne l'ID utilisateur si disponible
    pub fn user_id(&self) -> Option<&str> {
        self.user_id.as_deref()
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
        request = request.header("X-App-Id", &self.app_id);

        if let Some(ref token) = self.user_auth_token {
            request = request.header("X-User-Auth-Token", token);
        }

        // Ajouter les paramètres
        if method == "GET" {
            request = request.query(params);
        } else {
            request = request.form(params);
        }

        // Envoyer la requête
        let response = request.send().await?;
        self.handle_response(response).await
    }

    /// Traite la réponse HTTP
    async fn handle_response<T: DeserializeOwned>(&self, response: Response) -> Result<T> {
        let status = response.status();
        let status_code = status.as_u16();

        debug!("Response status: {}", status);

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            warn!("API error ({}): {}", status_code, error_text);
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
                    warn!("Qobuz API error: {}", message);
                    return Err(QobuzError::ApiError {
                        code: status_code,
                        message: message.to_string(),
                    });
                }
            }
        }

        // Parser la réponse
        serde_json::from_str(&text).map_err(|e| {
            warn!("Failed to parse response: {}", e);
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
        assert_eq!(api.app_id(), "test_app_id");
        assert!(api.auth_token().is_none());
    }

    #[test]
    fn test_set_auth_token() {
        let mut api = QobuzApi::new("test_app_id").unwrap();
        api.set_auth_token("test_token".to_string(), "user123".to_string());
        assert_eq!(api.auth_token(), Some("test_token"));
        assert_eq!(api.user_id(), Some("user123"));
    }

    #[test]
    fn test_set_format() {
        let mut api = QobuzApi::new("test_app_id").unwrap();
        api.set_format(AudioFormat::Flac_HiRes_96);
        assert_eq!(api.format(), AudioFormat::Flac_HiRes_96);
    }
}
