//! Module de signature MD5 pour les requêtes Qobuz
//!
//! Certaines requêtes Qobuz (notamment track/getFileUrl et userLibrary/*)
//! nécessitent une signature MD5 incluant le secret s4.

use md5::{Digest, Md5};
use std::time::{SystemTime, UNIX_EPOCH};

/// Génère un timestamp Unix actuel
///
/// # Returns
///
/// Timestamp Unix sous forme de string (integer, sans décimales)
///
/// # Exemple
///
/// ```
/// use pmoqobuz::api::signing::get_timestamp;
/// let ts = get_timestamp();
/// println!("Timestamp: {}", ts);
/// ```
pub fn get_timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        .to_string()
}

/// Signe une requête track/getFileUrl
///
/// Reproduit la logique Python:
/// ```python
/// stringvalue = ("trackgetFileUrlformat_id" + fmt_id +
///                "intent" + intent +
///                "track_id" + track_id + ts)
/// stringvalue += self.s4
/// rq_sig = str(hashlib.md5(stringvalue).hexdigest())
/// ```
///
/// # Arguments
///
/// * `format_id` - ID du format audio (ex: "27")
/// * `intent` - Intention (typiquement "stream")
/// * `track_id` - ID de la track
/// * `timestamp` - Timestamp Unix
/// * `secret` - Secret s4 en bytes
///
/// # Returns
///
/// Signature MD5 hexadécimale
pub fn sign_track_get_file_url(
    format_id: &str,
    intent: &str,
    track_id: &str,
    timestamp: &str,
    secret: &[u8],
) -> String {
    let mut hasher = Md5::new();

    // Construction de la chaîne à hasher
    hasher.update(b"trackgetFileUrlformat_id");
    hasher.update(format_id.as_bytes());
    hasher.update(b"intent");
    hasher.update(intent.as_bytes());
    hasher.update(b"track_id");
    hasher.update(track_id.as_bytes());
    hasher.update(timestamp.as_bytes());
    hasher.update(secret);

    // Retourner le hash hexadécimal
    format!("{:x}", hasher.finalize())
}

/// Signe une requête userLibrary/getAlbumsList
///
/// Reproduit la logique Python:
/// ```python
/// r_sig = "userLibrarygetAlbumsList" + str(ts) + str(ka["sec"])
/// r_sig_hashed = hashlib.md5(r_sig.encode("utf-8")).hexdigest()
/// ```
///
/// # Arguments
///
/// * `timestamp` - Timestamp Unix
/// * `secret` - Secret s4 en bytes
///
/// # Returns
///
/// Signature MD5 hexadécimale
pub fn sign_userlib_get_albums(timestamp: &str, secret: &[u8]) -> String {
    let mut hasher = Md5::new();

    // Construction de la chaîne à hasher
    hasher.update(b"userLibrarygetAlbumsList");
    hasher.update(timestamp.as_bytes());
    hasher.update(secret);

    // Retourner le hash hexadécimal
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_timestamp() {
        let ts = get_timestamp();
        // Vérifier que c'est un nombre entier valide
        assert!(ts.parse::<u64>().is_ok());
        // Vérifier que c'est proche du temps actuel (>= 2024)
        assert!(ts.parse::<u64>().unwrap() > 1704067200); // 1er janvier 2024
    }

    #[test]
    fn test_sign_track_get_file_url() {
        let signature =
            sign_track_get_file_url("27", "stream", "12345", "1234567890", b"test_secret");

        // Vérifier que c'est un hash MD5 valide (32 caractères hex)
        assert_eq!(signature.len(), 32);
        assert!(signature.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_sign_userlib_get_albums() {
        let signature = sign_userlib_get_albums("1234567890", b"test_secret");

        // Vérifier que c'est un hash MD5 valide (32 caractères hex)
        assert_eq!(signature.len(), 32);
        assert!(signature.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_signature_consistency() {
        // La même entrée doit produire la même signature
        let sig1 = sign_track_get_file_url("27", "stream", "123", "100", b"secret");
        let sig2 = sign_track_get_file_url("27", "stream", "123", "100", b"secret");
        assert_eq!(sig1, sig2);

        // Des entrées différentes doivent produire des signatures différentes
        let sig3 = sign_track_get_file_url("6", "stream", "123", "100", b"secret");
        assert_ne!(sig1, sig3);
    }
}
