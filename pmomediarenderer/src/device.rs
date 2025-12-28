//! Définition du device MediaRenderer.

use once_cell::sync::Lazy;
use std::sync::Arc;

use crate::{
    avtransport::AVTTRANSPORT, connectionmanager::CONNECTIONMANAGER,
    renderingcontrol::RENDERINGCONTROL,
};
use pmoupnp::devices::Device;

/// Device MediaRenderer UPnP.
///
/// MediaRenderer audio-only conforme UPnP AV Architecture 1.0.
///
/// # Services inclus
///
/// - **AVTransport:1** : Contrôle de la lecture
/// - **RenderingControl:1** : Contrôle du volume et du mute
/// - **ConnectionManager:1** : Gestion des connexions
///
/// # Spécifications
///
/// - Device Type : `urn:schemas-upnp-org:device:MediaRenderer:1`
/// - Version : 1
/// - Manufacturer : PMOMusic
/// - Model : PMOMusic Audio Renderer
///
/// # Exemple
///
/// ```ignore
/// use pmomediarenderer::MEDIA_RENDERER;
/// use pmoupnp::UpnpModel;
///
/// // Créer une instance du renderer
/// let renderer_instance = MEDIA_RENDERER.create_instance();
///
/// // Accéder aux services
/// if let Some(avtransport) = renderer_instance.get_service("AVTransport") {
///     // Contrôler la lecture...
/// }
/// ```
pub static MEDIA_RENDERER: Lazy<Arc<Device>> = Lazy::new(|| {
    let mut device = Device::new_from_config(
        "PMO_MediaRenderer".to_string(),
        "MediaRenderer".to_string(),
        "Audio Renderer".to_string(),
    );

    device.set_model_description("UPnP AV MediaRenderer for audio streaming".to_string());

    // Ajouter les trois services obligatoires
    device
        .add_service(Arc::clone(&AVTTRANSPORT))
        .expect("Failed to add AVTransport service");

    device
        .add_service(Arc::clone(&RENDERINGCONTROL))
        .expect("Failed to add RenderingControl service");

    device
        .add_service(Arc::clone(&CONNECTIONMANAGER))
        .expect("Failed to add ConnectionManager service");

    Arc::new(device)
});
