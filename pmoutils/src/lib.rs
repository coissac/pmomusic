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