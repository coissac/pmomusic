use crate::{RendererInfo, UpnpMediaServer};

pub mod manager;
pub mod upnp_discovery;
pub mod upnp_provider;
pub mod chromecast_discovery;
pub mod arylic;

/// Fournit les descriptions haut niveau à partir d’un endpoint découvert.
/// L’implémentation pourra, plus tard, faire un HTTP GET sur `location`
/// et parser la description pour remplir RendererInfo / MediaServerInfo.
pub trait DeviceDescriptionProvider: Send + Sync {
    /// Construit un RendererInfo pour cet endpoint, ou None s’il
    /// ne correspond pas à un renderer audio intéressant.
    fn build_renderer_info(
        &self,
        udn: &str,
        location: &str,
        server_header: &str,
    ) -> Option<RendererInfo>;

    /// Construit un MediaServerInfo pour cet endpoint, ou None s’il
    /// ne correspond pas à un media server (ou pas intéressant).
    fn build_server_info(
        &self,
        udn: &str,
        location: &str,
        server_header: &str,
    ) -> Option<UpnpMediaServer>;
}