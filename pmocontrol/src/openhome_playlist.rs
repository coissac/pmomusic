/// Snapshot de la playlist native OpenHome pour un renderer donné.
#[cfg_attr(feature = "pmoserver", derive(serde::Serialize, utoipa::ToSchema))]
#[derive(Debug, Clone)]
pub struct OpenHomePlaylistSnapshot {
    /// ID du renderer concerné.
    pub renderer_id: String,
    /// ID courant dans la playlist (si connu).
    pub current_id: Option<u32>,
    /// Tracks présents dans la playlist native.
    pub tracks: Vec<OpenHomePlaylistTrack>,
}

/// Représentation d'un track OpenHome tel qu'exposé par la playlist native.
#[cfg_attr(feature = "pmoserver", derive(serde::Serialize, utoipa::ToSchema))]
#[derive(Debug, Clone)]
pub struct OpenHomePlaylistTrack {
    /// ID interne OpenHome du track.
    pub id: u32,
    /// URI de lecture.
    pub uri: String,
    /// Titre (optionnel si non fourni par le renderer).
    pub title: Option<String>,
    /// Artiste (optionnel).
    pub artist: Option<String>,
    /// Album (optionnel).
    pub album: Option<String>,
    /// URI de pochette (optionnelle).
    pub album_art_uri: Option<String>,
}
