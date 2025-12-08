//! Module d'authentification pour l'API Qobuz

use super::QobuzApi;
use crate::error::{QobuzError, Result};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

/// Réponse de l'endpoint /user/login
#[derive(Debug, Deserialize)]
struct LoginResponse {
    user: UserInfo,
    user_auth_token: String,
}

/// Informations utilisateur retournées par l'API
#[derive(Debug, Deserialize)]
struct UserInfo {
    #[serde(deserialize_with = "crate::models::deserialize_id")]
    id: String,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    firstname: Option<String>,
    #[serde(default)]
    lastname: Option<String>,
    credential: CredentialInfo,
}

/// Informations sur les credentials de l'utilisateur
#[derive(Debug, Deserialize)]
struct CredentialInfo {
    #[serde(default)]
    parameters: Option<CredentialParameters>,
}

/// Paramètres du niveau d'abonnement
#[derive(Debug, Deserialize)]
struct CredentialParameters {
    #[serde(default)]
    short_label: Option<String>,
}

/// Informations d'authentification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthInfo {
    /// Token d'authentification
    pub token: String,
    /// ID utilisateur
    pub user_id: String,
    /// Label de l'abonnement (ex: "Studio", "Hi-Fi", etc.)
    pub subscription_label: Option<String>,
}

impl QobuzApi {
    /// Authentifie l'utilisateur avec username et password
    ///
    /// # Arguments
    ///
    /// * `username` - Email ou nom d'utilisateur Qobuz
    /// * `password` - Mot de passe
    ///
    /// # Returns
    ///
    /// Retourne les informations d'authentification si le login est réussi
    ///
    /// # Errors
    ///
    /// * `QobuzError::Unauthorized` - Credentials invalides
    /// * `QobuzError::SubscriptionRequired` - Compte gratuit (non éligible)
    pub async fn login(&mut self, username: &str, password: &str) -> Result<AuthInfo> {
        info!("Attempting to login to Qobuz as {}", username);

        let params = [("username", username), ("password", password)];

        let response: LoginResponse = self.post("/user/login", &params).await?;

        // Vérifier que l'utilisateur a un abonnement valide
        if response.user.credential.parameters.is_none() {
            return Err(QobuzError::SubscriptionRequired(
                "Free accounts are not eligible for streaming".to_string(),
            ));
        }

        let user_id = response.user.id;
        let subscription_label = response
            .user
            .credential
            .parameters
            .and_then(|p| p.short_label);

        debug!(
            "Login successful - User ID: {}, Subscription: {:?}",
            user_id, subscription_label
        );

        // Stocker les informations d'authentification
        self.set_auth_token(response.user_auth_token.clone(), user_id.clone());

        Ok(AuthInfo {
            token: response.user_auth_token,
            user_id,
            subscription_label,
        })
    }

    /// Vérifie si le client est authentifié
    pub fn is_authenticated(&self) -> bool {
        self.user_auth_token.is_some() && self.user_id.is_some()
    }

    /// Déconnecte l'utilisateur
    pub fn logout(&mut self) {
        debug!("Logging out");
        self.user_auth_token = None;
        self.user_id = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_authenticated() {
        let mut api = QobuzApi::new("test_app_id").unwrap();
        assert!(!api.is_authenticated());

        api.set_auth_token("token".to_string(), "user123".to_string());
        assert!(api.is_authenticated());

        api.logout();
        assert!(!api.is_authenticated());
    }
}
