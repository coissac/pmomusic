//! Définition du device MediaServer.

use once_cell::sync::Lazy;
use std::sync::Arc;

use crate::{connectionmanager::CONNECTIONMANAGER, contentdirectory::CONTENTDIRECTORY};
use pmoupnp::devices::Device;

/// Device MediaServer UPnP.
///
/// MediaServer conforme UPnP AV Architecture 1.0.
///
/// # Services inclus
///
/// - **ContentDirectory:1** : Gestion du contenu et navigation
/// - **ConnectionManager:1** : Gestion des connexions
///
/// # Spécifications
///
/// - Device Type : `urn:schemas-upnp-org:device:MediaServer:1`
/// - Version : 1
/// - Manufacturer : PMOMusic
/// - Model : PMOMusic Media Server
///
/// # Exemple
///
/// ```ignore
/// use pmomediaserver::MEDIA_SERVER;
/// use pmoupnp::UpnpModel;
///
/// // Créer une instance du server
/// let server_instance = MEDIA_SERVER.create_instance();
///
/// // Accéder aux services
/// if let Some(content_directory) = server_instance.get_service("ContentDirectory") {
///     // Gérer le contenu...
/// }
/// ```
pub static MEDIA_SERVER: Lazy<Arc<Device>> = Lazy::new(|| {
    let mut device = Device::new_from_config(
        "PMO_MediaServer".to_string(),
        "MediaServer".to_string(),
        "Media Server".to_string(),
    );

    device.set_model_description("UPnP AV MediaServer for audio streaming".to_string());

    // Ajouter les deux services obligatoires
    device
        .add_service(Arc::clone(&CONTENTDIRECTORY))
        .expect("Failed to add ContentDirectory service");

    device
        .add_service(Arc::clone(&CONNECTIONMANAGER))
        .expect("Failed to add ConnectionManager service");

    Arc::new(device)
});
