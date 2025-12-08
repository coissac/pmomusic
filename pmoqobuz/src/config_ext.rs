//! Extension pour intégrer la configuration Qobuz dans pmoconfig
//!
//! Ce module fournit le trait `QobuzConfigExt` qui permet d'ajouter facilement
//! des méthodes de gestion des credentials Qobuz à pmoconfig::Config.

use anyhow::{anyhow, Result};
use pmoconfig::Config;
use serde_yaml::Value;

/// Trait d'extension pour gérer la configuration Qobuz dans pmoconfig
///
/// Ce trait étend `pmoconfig::Config` avec des méthodes spécifiques
/// aux credentials et paramètres Qobuz.
///
/// # Exemple
///
/// ```rust,ignore
/// use pmoconfig::get_config;
/// use pmoqobuz::QobuzConfigExt;
///
/// let config = get_config();
/// let (username, password) = config.get_qobuz_credentials()?;
/// println!("Qobuz user: {}", username);
/// ```
pub trait QobuzConfigExt {
    /// Récupère le nom d'utilisateur Qobuz depuis la configuration
    ///
    /// # Returns
    ///
    /// Le nom d'utilisateur (email) configuré pour Qobuz
    ///
    /// # Errors
    ///
    /// Retourne une erreur si le nom d'utilisateur n'est pas configuré
    fn get_qobuz_username(&self) -> Result<String>;

    /// Définit le nom d'utilisateur Qobuz dans la configuration
    ///
    /// # Arguments
    ///
    /// * `username` - Le nom d'utilisateur (email) Qobuz
    fn set_qobuz_username(&self, username: &str) -> Result<()>;

    /// Récupère le mot de passe Qobuz depuis la configuration
    ///
    /// # Returns
    ///
    /// Le mot de passe configuré pour Qobuz
    ///
    /// # Errors
    ///
    /// Retourne une erreur si le mot de passe n'est pas configuré
    fn get_qobuz_password(&self) -> Result<String>;

    /// Définit le mot de passe Qobuz dans la configuration
    ///
    /// # Arguments
    ///
    /// * `password` - Le mot de passe Qobuz
    fn set_qobuz_password(&self, password: &str) -> Result<()>;

    /// Récupère les credentials Qobuz (username et password)
    ///
    /// # Returns
    ///
    /// Un tuple (username, password) contenant les credentials Qobuz
    ///
    /// # Errors
    ///
    /// Retourne une erreur si l'un des credentials n'est pas configuré
    ///
    /// # Exemple
    ///
    /// ```rust,ignore
    /// use pmoconfig::get_config;
    /// use pmoqobuz::QobuzConfigExt;
    ///
    /// let config = get_config();
    /// match config.get_qobuz_credentials() {
    ///     Ok((username, password)) => {
    ///         println!("Credentials configured for: {}", username);
    ///     }
    ///     Err(e) => {
    ///         eprintln!("Qobuz credentials not configured: {}", e);
    ///     }
    /// }
    /// ```
    fn get_qobuz_credentials(&self) -> Result<(String, String)>;

    /// Récupère l'App ID Qobuz depuis la configuration
    ///
    /// # Returns
    ///
    /// L'App ID configuré pour Qobuz, ou None si non configuré
    ///
    /// # Note
    ///
    /// Si aucun App ID n'est configuré, le client utilisera soit le Spoofer
    /// pour en obtenir un dynamiquement, soit un App ID par défaut.
    fn get_qobuz_appid(&self) -> Result<Option<String>>;

    /// Définit l'App ID Qobuz dans la configuration
    ///
    /// # Arguments
    ///
    /// * `appid` - L'App ID Qobuz (ex: "1401488693436528")
    fn set_qobuz_appid(&self, appid: &str) -> Result<()>;

    /// Récupère le secret Qobuz depuis la configuration
    ///
    /// # Returns
    ///
    /// Le secret encodé en base64, ou None si non configuré
    ///
    /// # Note
    ///
    /// Le secret est la valeur `configvalue` du code Python.
    /// Il est décodé et XORé avec l'App ID pour obtenir le secret `s4`
    /// utilisé pour signer les requêtes sensibles.
    ///
    /// Si aucun secret n'est configuré, le client utilisera le Spoofer
    /// pour en obtenir un dynamiquement.
    fn get_qobuz_secret(&self) -> Result<Option<String>>;

    /// Définit le secret Qobuz dans la configuration
    ///
    /// # Arguments
    ///
    /// * `secret` - Le secret encodé en base64 (configvalue)
    fn set_qobuz_secret(&self, secret: &str) -> Result<()>;

    /// Récupère le token d'authentification depuis la configuration
    ///
    /// # Returns
    ///
    /// Le token d'authentification, ou None si non configuré ou expiré
    fn get_qobuz_auth_token(&self) -> Result<Option<String>>;

    /// Récupère l'ID utilisateur depuis la configuration
    ///
    /// # Returns
    ///
    /// L'ID utilisateur, ou None si non configuré
    fn get_qobuz_user_id(&self) -> Result<Option<String>>;

    /// Récupère le timestamp d'expiration du token
    ///
    /// # Returns
    ///
    /// Le timestamp d'expiration (Unix timestamp), ou None si non configuré
    fn get_qobuz_token_expires_at(&self) -> Result<Option<u64>>;

    /// Récupère le label de l'abonnement depuis la configuration
    fn get_qobuz_subscription_label(&self) -> Result<Option<String>>;

    /// Sauvegarde les informations d'authentification dans la configuration
    ///
    /// # Arguments
    ///
    /// * `token` - Le token d'authentification
    /// * `user_id` - L'ID utilisateur
    /// * `subscription_label` - Le label de l'abonnement (optionnel)
    /// * `expires_at` - Timestamp d'expiration (Unix timestamp)
    fn set_qobuz_auth_info(
        &self,
        token: &str,
        user_id: &str,
        subscription_label: Option<&str>,
        expires_at: u64,
    ) -> Result<()>;

    /// Supprime les informations d'authentification de la configuration
    fn clear_qobuz_auth_info(&self) -> Result<()>;

    /// Vérifie si le token d'authentification est encore valide
    ///
    /// # Returns
    ///
    /// true si un token existe et n'est pas expiré, false sinon
    fn is_qobuz_auth_valid(&self) -> bool;

    /// Récupère le répertoire de cache Qobuz
    ///
    /// # Returns
    ///
    /// Le chemin absolu du répertoire de cache, créé s'il n'existe pas
    fn get_qobuz_cache_dir(&self) -> Result<String>;

    /// Définit le répertoire de cache Qobuz
    fn set_qobuz_cache_dir(&self, directory: String) -> Result<()>;
}

impl QobuzConfigExt for Config {
    fn get_qobuz_username(&self) -> Result<String> {
        match self.get_value(&["accounts", "qobuz", "username"])? {
            Value::String(s) => Ok(s),
            _ => Err(anyhow!("Qobuz username not configured")),
        }
    }

    fn set_qobuz_username(&self, username: &str) -> Result<()> {
        self.set_value(
            &["accounts", "qobuz", "username"],
            Value::String(username.to_string()),
        )
    }

    fn get_qobuz_password(&self) -> Result<String> {
        match self.get_value(&["accounts", "qobuz", "password"])? {
            Value::String(s) => {
                // Déchiffrement automatique si le mot de passe est chiffré
                pmoconfig::encryption::get_password(&s)
                    .map_err(|e| anyhow!("Failed to decrypt password: {}", e))
            }
            _ => Err(anyhow!("Qobuz password not configured")),
        }
    }

    fn set_qobuz_password(&self, password: &str) -> Result<()> {
        self.set_value(
            &["accounts", "qobuz", "password"],
            Value::String(password.to_string()),
        )
    }

    fn get_qobuz_credentials(&self) -> Result<(String, String)> {
        let username = self.get_qobuz_username()?;
        let password = self.get_qobuz_password()?;
        Ok((username, password))
    }

    fn get_qobuz_appid(&self) -> Result<Option<String>> {
        match self.get_value(&["accounts", "qobuz", "appid"]) {
            Ok(Value::String(s)) if !s.is_empty() => Ok(Some(s)),
            Ok(Value::String(_)) => Ok(None), // Empty string
            Ok(_) => Ok(None), // Wrong type
            Err(_) => Ok(None), // Not configured
        }
    }

    fn set_qobuz_appid(&self, appid: &str) -> Result<()> {
        self.set_value(
            &["accounts", "qobuz", "appid"],
            Value::String(appid.to_string()),
        )
    }

    fn get_qobuz_secret(&self) -> Result<Option<String>> {
        match self.get_value(&["accounts", "qobuz", "secret"]) {
            Ok(Value::String(s)) if !s.is_empty() => Ok(Some(s)),
            Ok(Value::String(_)) => Ok(None), // Empty string
            Ok(_) => Ok(None), // Wrong type
            Err(_) => Ok(None), // Not configured
        }
    }

    fn set_qobuz_secret(&self, secret: &str) -> Result<()> {
        self.set_value(
            &["accounts", "qobuz", "secret"],
            Value::String(secret.to_string()),
        )
    }

    fn get_qobuz_auth_token(&self) -> Result<Option<String>> {
        match self.get_value(&["accounts", "qobuz", "auth_token"]) {
            Ok(Value::String(s)) if !s.is_empty() => Ok(Some(s)),
            Ok(Value::String(_)) => Ok(None), // Empty string
            Ok(_) => Ok(None),                 // Wrong type
            Err(_) => Ok(None),                // Not configured
        }
    }

    fn get_qobuz_user_id(&self) -> Result<Option<String>> {
        match self.get_value(&["accounts", "qobuz", "user_id"]) {
            Ok(Value::String(s)) if !s.is_empty() => Ok(Some(s)),
            Ok(Value::String(_)) => Ok(None), // Empty string
            Ok(_) => Ok(None),                 // Wrong type
            Err(_) => Ok(None),                // Not configured
        }
    }

    fn get_qobuz_token_expires_at(&self) -> Result<Option<u64>> {
        match self.get_value(&["accounts", "qobuz", "token_expires_at"]) {
            Ok(Value::Number(n)) if n.is_u64() => Ok(Some(n.as_u64().unwrap())),
            Ok(Value::Number(n)) if n.is_i64() => Ok(Some(n.as_i64().unwrap() as u64)),
            Ok(_) => Ok(None),  // Wrong type
            Err(_) => Ok(None), // Not configured
        }
    }

    fn get_qobuz_subscription_label(&self) -> Result<Option<String>> {
        match self.get_value(&["accounts", "qobuz", "subscription_label"]) {
            Ok(Value::String(s)) if !s.is_empty() => Ok(Some(s)),
            Ok(Value::String(_)) => Ok(None), // Empty string
            Ok(_) => Ok(None),                 // Wrong type
            Err(_) => Ok(None),                // Not configured
        }
    }

    fn set_qobuz_auth_info(
        &self,
        token: &str,
        user_id: &str,
        subscription_label: Option<&str>,
        expires_at: u64,
    ) -> Result<()> {
        self.set_value(
            &["accounts", "qobuz", "auth_token"],
            Value::String(token.to_string()),
        )?;
        self.set_value(
            &["accounts", "qobuz", "user_id"],
            Value::String(user_id.to_string()),
        )?;
        self.set_value(
            &["accounts", "qobuz", "token_expires_at"],
            Value::Number(serde_yaml::Number::from(expires_at)),
        )?;

        if let Some(label) = subscription_label {
            self.set_value(
                &["accounts", "qobuz", "subscription_label"],
                Value::String(label.to_string()),
            )?;
        }

        Ok(())
    }

    fn clear_qobuz_auth_info(&self) -> Result<()> {
        // On ne propage pas les erreurs car les valeurs peuvent ne pas exister
        let _ = self.set_value(&["accounts", "qobuz", "auth_token"], Value::String(String::new()));
        let _ = self.set_value(&["accounts", "qobuz", "user_id"], Value::String(String::new()));
        let _ = self.set_value(
            &["accounts", "qobuz", "token_expires_at"],
            Value::Number(serde_yaml::Number::from(0)),
        );
        let _ = self.set_value(
            &["accounts", "qobuz", "subscription_label"],
            Value::String(String::new()),
        );
        Ok(())
    }

    fn is_qobuz_auth_valid(&self) -> bool {
        // Vérifier si un token existe
        if self.get_qobuz_auth_token().ok().flatten().is_none() {
            return false;
        }

        // Vérifier si le token n'est pas expiré
        if let Ok(Some(expires_at)) = self.get_qobuz_token_expires_at() {
            use std::time::{SystemTime, UNIX_EPOCH};
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();

            now < expires_at
        } else {
            false
        }
    }

    fn get_qobuz_cache_dir(&self) -> Result<String> {
        self.get_managed_dir(&["host", "qobuz_cache", "directory"], "cache_qobuz")
    }

    fn set_qobuz_cache_dir(&self, directory: String) -> Result<()> {
        self.set_managed_dir(&["host", "qobuz_cache", "directory"], directory)
    }
}
