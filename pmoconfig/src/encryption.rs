//! Module de chiffrement des mots de passe basé sur l'UUID de la machine
//!
//! Ce module fournit un chiffrement transparent des mots de passe dans la
//! configuration. La clé de chiffrement est dérivée de l'UUID matériel de
//! la machine, ce qui rend le fichier config non-portable mais protégé.

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use anyhow::{anyhow, Result};
use base64::Engine;
use sha2::{Digest, Sha256};
use std::process::Command;

/// Préfixe pour identifier les mots de passe chiffrés
const ENCRYPTED_PREFIX: &str = "encrypted:";

/// Récupère l'UUID matériel de la machine
///
/// Sur macOS, utilise `ioreg -d2 -c IOPlatformExpertDevice`
/// Sur Linux, utilise `/etc/machine-id` ou `/var/lib/dbus/machine-id`
/// Sur Windows, utilise `wmic csproduct get UUID`
fn get_machine_uuid() -> Result<String> {
    #[cfg(target_os = "macos")]
    {
        let output = Command::new("ioreg")
            .args(["-d2", "-c", "IOPlatformExpertDevice"])
            .output()?;

        let output_str = String::from_utf8_lossy(&output.stdout);

        // Chercher la ligne contenant IOPlatformUUID
        for line in output_str.lines() {
            if line.contains("IOPlatformUUID") {
                // Format: "IOPlatformUUID" = "XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX"
                if let Some(uuid) = line.split('"').nth(3) {
                    return Ok(uuid.to_string());
                }
            }
        }

        Err(anyhow!("Failed to extract IOPlatformUUID from ioreg"))
    }

    #[cfg(target_os = "linux")]
    {
        use std::fs;

        // Essayer /etc/machine-id en premier
        if let Ok(uuid) = fs::read_to_string("/etc/machine-id") {
            return Ok(uuid.trim().to_string());
        }

        // Fallback sur /var/lib/dbus/machine-id
        if let Ok(uuid) = fs::read_to_string("/var/lib/dbus/machine-id") {
            return Ok(uuid.trim().to_string());
        }

        Err(anyhow!("Failed to read machine-id"))
    }

    #[cfg(target_os = "windows")]
    {
        let output = Command::new("wmic")
            .args(["csproduct", "get", "UUID"])
            .output()?;

        let output_str = String::from_utf8_lossy(&output.stdout);

        // La deuxième ligne contient l'UUID
        if let Some(uuid) = output_str.lines().nth(1) {
            return Ok(uuid.trim().to_string());
        }

        Err(anyhow!("Failed to extract UUID from wmic"))
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        Err(anyhow!("Unsupported platform for machine UUID extraction"))
    }
}

/// Dérive une clé de chiffrement AES-256 à partir de l'UUID de la machine
fn derive_key() -> Result<[u8; 32]> {
    let machine_uuid = get_machine_uuid()?;

    // Utiliser SHA-256 pour dériver une clé de 256 bits
    let mut hasher = Sha256::new();
    hasher.update(machine_uuid.as_bytes());
    hasher.update(b"pmomusic-config-encryption-v1"); // Salt pour différencier

    let result = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&result);

    Ok(key)
}

/// Chiffre un mot de passe avec la clé dérivée de la machine
///
/// # Arguments
///
/// * `password` - Le mot de passe en clair
///
/// # Returns
///
/// Le mot de passe chiffré au format "encrypted:BASE64"
/// Le format encodé est : nonce(12 bytes) + ciphertext
///
/// # Example
///
/// ```rust,ignore
/// let encrypted = encrypt_password("my_password")?;
/// // encrypted = "encrypted:SGVsbG8gV29ybGQh..."
/// ```
pub fn encrypt_password(password: &str) -> Result<String> {
    let key = derive_key()?;
    let cipher =
        Aes256Gcm::new_from_slice(&key).map_err(|e| anyhow!("Failed to create cipher: {}", e))?;

    // Nonce de 96 bits (12 bytes) - dérivé du mot de passe pour avoir
    // un chiffrement déterministe (même password = même ciphertext)
    // Cela permet d'éviter de modifier le fichier config si le password n'a pas changé
    let mut nonce_bytes = [0u8; 12];
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    hasher.update(b"pmomusic-nonce-v1");
    let nonce_hash = hasher.finalize();
    nonce_bytes.copy_from_slice(&nonce_hash[..12]);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, password.as_bytes())
        .map_err(|e| anyhow!("Encryption failed: {}", e))?;

    // Stocker nonce + ciphertext ensemble
    let mut combined = Vec::with_capacity(12 + ciphertext.len());
    combined.extend_from_slice(&nonce_bytes);
    combined.extend_from_slice(&ciphertext);

    Ok(format!(
        "{}{}",
        ENCRYPTED_PREFIX,
        base64::engine::general_purpose::STANDARD.encode(&combined)
    ))
}

/// Déchiffre un mot de passe avec la clé dérivée de la machine
///
/// # Arguments
///
/// * `encrypted` - Le mot de passe chiffré au format "encrypted:BASE64"
///
/// # Returns
///
/// Le mot de passe en clair
///
/// # Errors
///
/// Retourne une erreur si le format est invalide ou si le déchiffrement échoue
///
/// # Example
///
/// ```rust,ignore
/// let password = decrypt_password("encrypted:SGVsbG8gV29ybGQh...")?;
/// ```
pub fn decrypt_password(encrypted: &str) -> Result<String> {
    // Vérifier le préfixe
    let base64_data = encrypted
        .strip_prefix(ENCRYPTED_PREFIX)
        .ok_or_else(|| anyhow!("Invalid encrypted password format (missing prefix)"))?;

    let key = derive_key()?;
    let cipher =
        Aes256Gcm::new_from_slice(&key).map_err(|e| anyhow!("Failed to create cipher: {}", e))?;

    let ciphertext = base64::engine::general_purpose::STANDARD
        .decode(base64_data)
        .map_err(|e| anyhow!("Invalid base64: {}", e))?;

    // Dériver le même nonce (on ne peut pas le stocker car on veut un chiffrement déterministe)
    // On va essayer de déchiffrer avec tous les nonces possibles... non, ça ne marche pas.
    // Problème : on ne peut pas dériver le nonce du mot de passe chiffré car on ne connaît pas le plaintext.

    // Solution : stocker le nonce avec le ciphertext
    // Format: nonce(12 bytes) + ciphertext
    if ciphertext.len() < 12 {
        return Err(anyhow!("Invalid ciphertext (too short)"));
    }

    let nonce = Nonce::from_slice(&ciphertext[..12]);
    let actual_ciphertext = &ciphertext[12..];

    let plaintext = cipher
        .decrypt(nonce, actual_ciphertext)
        .map_err(|e| anyhow!("Decryption failed (wrong machine or corrupted data): {}", e))?;

    String::from_utf8(plaintext).map_err(|e| anyhow!("Invalid UTF-8: {}", e))
}

/// Vérifie si une valeur est un mot de passe chiffré
///
/// # Arguments
///
/// * `value` - La valeur à tester
///
/// # Returns
///
/// `true` si la valeur commence par "encrypted:", `false` sinon
pub fn is_encrypted(value: &str) -> bool {
    value.starts_with(ENCRYPTED_PREFIX)
}

/// Obtient le mot de passe en clair, qu'il soit chiffré ou non
///
/// Cette fonction gère automatiquement la détection du format :
/// - Si le mot de passe commence par "encrypted:", il est déchiffré
/// - Sinon, il est retourné tel quel (plaintext)
///
/// # Arguments
///
/// * `value` - Le mot de passe (chiffré ou non)
///
/// # Returns
///
/// Le mot de passe en clair
///
/// # Example
///
/// ```rust,ignore
/// // Plaintext
/// let password = get_password("my_password")?;
/// // password = "my_password"
///
/// // Encrypted
/// let password = get_password("encrypted:SGVsbG8...")?;
/// // password = "decrypted_password"
/// ```
pub fn get_password(value: &str) -> Result<String> {
    if is_encrypted(value) {
        decrypt_password(value)
    } else {
        Ok(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_machine_uuid() {
        let uuid = get_machine_uuid();
        assert!(uuid.is_ok(), "Should be able to get machine UUID");
        println!("Machine UUID: {}", uuid.unwrap());
    }

    #[test]
    fn test_encrypt_decrypt() {
        let password = "SuperSecret123!";

        let encrypted = encrypt_password(password).unwrap();
        assert!(encrypted.starts_with(ENCRYPTED_PREFIX));
        assert_ne!(encrypted, password);

        let decrypted = decrypt_password(&encrypted).unwrap();
        assert_eq!(decrypted, password);
    }

    #[test]
    fn test_is_encrypted() {
        assert!(is_encrypted("encrypted:SGVsbG8="));
        assert!(!is_encrypted("plaintext"));
        assert!(!is_encrypted(""));
    }

    #[test]
    fn test_get_password() {
        // Plaintext
        let password = get_password("plaintext").unwrap();
        assert_eq!(password, "plaintext");

        // Encrypted
        let encrypted = encrypt_password("secret").unwrap();
        let password = get_password(&encrypted).unwrap();
        assert_eq!(password, "secret");
    }
}
