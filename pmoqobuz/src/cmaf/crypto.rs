use aes::cipher::{BlockDecryptMut, KeyIvInit, StreamCipher};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hkdf::Hkdf;
use md5::{Digest, Md5};
use sha2::Sha256;

use super::error::CmafError;

type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;
type Aes128Ctr = ctr::Ctr128BE<aes::Aes128>;

/// Seed publique extraite du bundle web Qobuz. Valeur IKM pour HKDF.
pub const CMAF_SEED: &str = "abb21364945c0583309667d13ca3d93a";

fn hex_decode(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(0))
        .collect()
}

/// Dérive la clé de session 16 octets depuis le champ `infos` de session/start.
///
/// Format infos : `"salt_b64url.info_b64url"`
/// `seed` est le CMAF_SEED hex-encodé utilisé comme IKM HKDF.
pub fn derive_session_key(seed: &str, infos: &str) -> Result<[u8; 16], CmafError> {
    let parts: Vec<&str> = infos.split('.').collect();
    if parts.len() < 2 {
        return Err(CmafError::InvalidInfos(
            "session infos doit avoir au moins 2 parties séparées par des points".into(),
        ));
    }

    let salt = URL_SAFE_NO_PAD.decode(parts[0])?;
    let info = URL_SAFE_NO_PAD.decode(parts[1])?;
    let ikm = hex_decode(seed);

    let hk = Hkdf::<Sha256>::new(Some(&salt), &ikm);
    let mut okm = [0u8; 16];
    hk.expand(&info, &mut okm).map_err(|_| CmafError::HkdfExpand)?;

    Ok(okm)
}

/// Déroule la clé de contenu par track avec la clé de session.
///
/// Format key_str : `"qbz-1.wrapped_key_b64url.iv_b64url"`
pub fn unwrap_content_key(session_key: &[u8; 16], key_str: &str) -> Result<[u8; 16], CmafError> {
    let parts: Vec<&str> = key_str.split('.').collect();
    if parts.len() < 3 {
        return Err(CmafError::InvalidKey(
            "key string doit avoir au moins 3 parties séparées par des points".into(),
        ));
    }

    let wrapped = URL_SAFE_NO_PAD.decode(parts[1])?;
    let iv = URL_SAFE_NO_PAD.decode(parts[2])?;

    if iv.len() != 16 {
        return Err(CmafError::InvalidKey(format!(
            "IV de dérobage doit faire 16 octets, reçu {}",
            iv.len()
        )));
    }

    let mut buf = wrapped.clone();
    let decrypted =
        Aes128CbcDec::new(session_key.into(), iv.as_slice().into())
            .decrypt_padded_mut::<aes::cipher::block_padding::Pkcs7>(&mut buf)
            .map_err(|e| CmafError::AesDecrypt(format!("AES-CBC unwrap échoué: {e}")))?;

    if decrypted.len() != 16 {
        return Err(CmafError::InvalidKey(format!(
            "clé déroullée doit faire 16 octets, obtenu {}",
            decrypted.len()
        )));
    }

    let mut key = [0u8; 16];
    key.copy_from_slice(decrypted);
    Ok(key)
}

/// Déchiffre une frame FLAC en place avec AES-128-CTR.
///
/// `iv_8` = IV 8 octets du segment UUID box, complété à zéro jusqu'à 16 octets.
pub fn decrypt_frame(content_key: &[u8; 16], iv_8: &[u8; 8], data: &mut [u8]) {
    let mut nonce = [0u8; 16];
    nonce[..8].copy_from_slice(iv_8);
    Aes128Ctr::new(content_key.into(), &nonce.into()).apply_keystream(data);
}

/// Calcule la signature MD5 pour les appels API CMAF de Qobuz.
///
/// Concatène method + paires clé-valeur triées + timestamp + seed,
/// puis retourne le digest MD5 hexadécimal minuscule.
pub fn compute_request_sig(
    method: &str,
    args: &std::collections::BTreeMap<&str, String>,
    timestamp: &str,
    seed: &str,
) -> String {
    let mut hasher = Md5::new();
    hasher.update(method.as_bytes());
    for (k, v) in args {
        hasher.update(k.as_bytes());
        hasher.update(v.as_bytes());
    }
    hasher.update(timestamp.as_bytes());
    hasher.update(seed.as_bytes());

    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_request_sig_deterministe() {
        let mut args = std::collections::BTreeMap::new();
        args.insert("profile", "qbz-1".to_string());
        let sig1 = compute_request_sig("sessionstart", &args, "1775500000", CMAF_SEED);
        let sig2 = compute_request_sig("sessionstart", &args, "1775500000", CMAF_SEED);
        assert_eq!(sig1.len(), 32);
        assert_eq!(sig1, sig2);
    }

    #[test]
    fn test_decrypt_frame_aller_retour() {
        let key = [1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
        let iv = [1u8, 2, 3, 4, 5, 6, 7, 8];
        let original = b"Hello FLAC frame data here!".to_vec();
        let mut data = original.clone();
        decrypt_frame(&key, &iv, &mut data);
        assert_ne!(data, original);
        // AES-CTR est son propre inverse
        decrypt_frame(&key, &iv, &mut data);
        assert_eq!(data, original);
    }

    #[test]
    fn test_derive_session_key_infos_invalide() {
        let result = derive_session_key(CMAF_SEED, "pas_de_point");
        assert!(result.is_err());
    }

    #[test]
    fn test_unwrap_content_key_format_invalide() {
        let key = [0u8; 16];
        let result = unwrap_content_key(&key, "seulement.deux");
        assert!(result.is_err());
    }
}
