//! # Handlers pour les actions ContentDirectory
//!
//! Ce module implémente les handlers UPnP pour les actions du service ContentDirectory.
//! Chaque handler fait le pont entre l'API UPnP et la logique métier dans [`ContentHandler`].
//!
//! ## Architecture
//!
//! ```text
//! UPnP Action (XML)
//!       ↓
//! Handler (ce module) - extraction des paramètres
//!       ↓
//! ContentHandler - logique métier
//!       ↓
//! Sources musicales
//! ```
//!
//! ## Handlers implémentés
//!
//! - [`browse_handler`] : Navigation dans la hiérarchie de contenu
//! - [`search_handler`] : Recherche dans les sources
//! - [`get_search_capabilities_handler`] : Capacités de recherche supportées
//! - [`get_sort_capabilities_handler`] : Capacités de tri supportées
//! - [`get_system_update_id_handler`] : ID de mise à jour du système

use crate::content_handler::ContentHandler;
use pmoupnp::actions::{ActionError, ActionHandler};
use pmoupnp::{action_handler, get, set};
use tracing::{debug, error, info};

/// Handler pour l'action Browse.
///
/// Navigue dans la hiérarchie de contenu (containers et items).
///
/// # Arguments UPnP
///
/// - `ObjectID` : ID de l'objet à parcourir ("0" pour la racine)
/// - `BrowseFlag` : "BrowseMetadata" ou "BrowseDirectChildren"
/// - `Filter` : Filtre de propriétés (non utilisé actuellement)
/// - `StartingIndex` : Index de départ pour la pagination
/// - `RequestedCount` : Nombre d'éléments demandés (0 = tous)
/// - `SortCriteria` : Critères de tri (non utilisé actuellement)
///
/// # Retours UPnP
///
/// - `Result` : XML DIDL-Lite contenant les résultats
/// - `NumberReturned` : Nombre d'éléments retournés
/// - `TotalMatches` : Nombre total d'éléments
/// - `UpdateID` : ID de mise à jour
pub fn browse_handler() -> ActionHandler {
    action_handler!(|data| {
        let mut data = data;
        debug!("📂 Browse handler called");

        let handler = ContentHandler::new();

        let object_id: String = get!(&data, "ObjectID", String);
        let browse_flag: String = get!(&data, "BrowseFlag", String);

        let starting_index: u32 = get!(
            &data,
            "StartingIndex",
            u32,
            "ContentDirectory::Browse misconfigured: 'StartingIndex' missing or not bound"
        );

        let requested_count: u32 = get!(
            &data,
            "RequestedCount",
            u32,
            "ContentDirectory::Browse misconfigured: 'RequestedCount' missing or not bound"
        );

        let _filter: String = get!(
            &data,
            "Filter",
            String,
            "ContentDirectory::Browse misconfigured: 'Filter' missing or not bound"
        );

        let _sort_criteria: String = get!(
            &data,
            "SortCriteria",
            String,
            "ContentDirectory::Browse misconfigured: 'SortCriteria' missing or not bound"
        );

        info!(
            "📂 Browse requested: object_id={} flag={} start={} count={}",
            object_id, browse_flag, starting_index, requested_count
        );

        // Appeler la logique métier
        let (didl, returned, total, update_id) = handler
            .browse(&object_id, &browse_flag, starting_index, requested_count)
            .await
            .map_err(|e| {
                error!("Browse failed: {}", e);
                ActionError::GeneralError(e)
            })?;

        // Définir les arguments de sortie
        set!(&mut data, "Result", didl);
        set!(&mut data, "NumberReturned", returned);
        set!(&mut data, "TotalMatches", total);
        set!(&mut data, "UpdateID", update_id);

        debug!(
            "✅ Browse completed: returned={}, total={}",
            returned, total
        );
        info!(
            "📂 Browse completed: object_id={} returned={} total={}",
            object_id, returned, total
        );
        Ok(data)
    })
}

/// Handler pour l'action Search.
///
/// Recherche du contenu dans les sources qui supportent la recherche.
///
/// # Arguments UPnP
///
/// - `ContainerID` : ID du container dans lequel rechercher
/// - `SearchCriteria` : Critères de recherche UPnP
/// - `Filter` : Filtre de propriétés (non utilisé)
/// - `StartingIndex` : Index de départ
/// - `RequestedCount` : Nombre demandé
/// - `SortCriteria` : Critères de tri (non utilisé)
///
/// # Retours UPnP
///
/// - `Result` : XML DIDL-Lite
/// - `NumberReturned` : Nombre retourné
/// - `TotalMatches` : Total
/// - `UpdateID` : ID de mise à jour
pub fn search_handler() -> ActionHandler {
    action_handler!(|data| {
        let mut data = data;
        debug!("🔍 Search handler called");

        let handler = ContentHandler::new();

        let container_id: String = get!(&data, "ContainerID", String);
        let search_criteria: String = get!(&data, "SearchCriteria", String);
        let _filter: String = get!(&data, "Filter", String);
        let _starting_index: u32 = get!(&data, "StartingIndex", u32);
        let _requested_count: u32 = get!(&data, "RequestedCount", u32);
        let _sort_criteria: String = get!(&data, "SortCriteria", String);

        let (didl, returned, total, update_id) = handler
            .search(&container_id, &search_criteria)
            .await
            .map_err(|e| {
                error!("Search failed: {}", e);
                ActionError::GeneralError(e)
            })?;

        // Définir les sorties
        set!(&mut data, "Result", didl);
        set!(&mut data, "NumberReturned", returned);
        set!(&mut data, "TotalMatches", total);
        set!(&mut data, "UpdateID", update_id);

        debug!(
            "✅ Search completed: returned={}, total={}",
            returned, total
        );
        Ok(data)
    })
}

/// Handler pour GetSearchCapabilities.
///
/// Retourne les capacités de recherche supportées.
///
/// # Retours UPnP
///
/// - `SearchCaps` : Chaîne de capacités séparées par virgules
pub fn get_search_capabilities_handler() -> ActionHandler {
    action_handler!(|data| {
        let mut data = data;
        debug!("🔍 GetSearchCapabilities handler called");

        let handler = ContentHandler::new();
        let capabilities = handler.get_search_capabilities().await;

        set!(&mut data, "SearchCaps", capabilities.clone());

        debug!("✅ SearchCapabilities: {}", capabilities);
        Ok(data)
    })
}

/// Handler pour GetSortCapabilities.
///
/// Retourne les capacités de tri supportées.
///
/// # Retours UPnP
///
/// - `SortCaps` : Chaîne de capacités séparées par virgules
pub fn get_sort_capabilities_handler() -> ActionHandler {
    action_handler!(|data| {
        let mut data = data;
        debug!("📊 GetSortCapabilities handler called");

        let handler = ContentHandler::new();
        let capabilities = handler.get_sort_capabilities().await;

        set!(&mut data, "SortCaps", capabilities.clone());

        debug!("✅ SortCapabilities: {}", capabilities);
        Ok(data)
    })
}

/// Handler pour GetSystemUpdateID.
///
/// Retourne l'ID de mise à jour global du système.
/// Cet ID change quand le contenu disponible change.
///
/// # Retours UPnP
///
/// - `Id` : ID de mise à jour (entier non signé)
pub fn get_system_update_id_handler() -> ActionHandler {
    action_handler!(|data| {
        let mut data = data;
        debug!("🔄 GetSystemUpdateID handler called");

        let handler = ContentHandler::new();
        let update_id = handler.get_system_update_id().await;

        set!(&mut data, "Id", update_id);

        debug!("✅ SystemUpdateID: {}", update_id);
        Ok(data)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handlers_creation() {
        // Vérifier que tous les handlers se créent sans erreur
        let _ = browse_handler();
        let _ = search_handler();
        let _ = get_search_capabilities_handler();
        let _ = get_sort_capabilities_handler();
        let _ = get_system_update_id_handler();
    }
}
