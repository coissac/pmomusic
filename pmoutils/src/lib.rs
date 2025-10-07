/// Utilitaires pour la gestion des adresses IP réseau.
///
/// Ce module fournit des fonctions pour détecter et lister les adresses IP
/// des interfaces réseau locales de la machine.
///
/// # Fonctions principales
///
/// - [`guess_local_ip`] : Devine l'adresse IP locale utilisée pour les connexions sortantes
///
/// # Examples
///
/// ```
/// use votre_crate::guess_local_ip;
///
/// let ip = guess_local_ip();
/// println!("Adresse IP locale: {}", ip);
/// ```
mod ip_utils;

pub use ip_utils::guess_local_ip;

/// Retourne une chaîne décrivant le système d'exploitation et sa version.
///
/// Utilise la crate `os_info` pour obtenir de manière portable et fiable
/// les informations sur le système d'exploitation courant.
///
/// # Format
/// - macOS: "macOS/15.1" ou "Mac OS/10.15.7"
/// - Linux: "Linux/6.5.0" ou "Ubuntu/22.04"
/// - Windows: "Windows/10.0.19045"
/// - Autre: "{OS}/Unknown"
///
/// # Exemples
///
/// ```
/// use pmoutils::get_os_string;
///
/// let os = get_os_string();
/// println!("OS: {}", os); // Ex: "Linux/6.5.0"
/// ```
pub fn get_os_string() -> String {
    let info = os_info::get();
    let os_type = format!("{:?}", info.os_type());

    // Obtenir la version si disponible
    let version = info.version();
    if version != &os_info::Version::Unknown {
        format!("{}/{}", os_type, version)
    } else {
        format!("{}/Unknown", os_type)
    }
}