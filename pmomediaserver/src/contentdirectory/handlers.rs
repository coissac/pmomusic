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

use pmoupnp::action_handler;
use pmoupnp::actions::{ActionHandler, ActionError};
use pmoupnp::variable_types::StateValue;
use crate::content_handler::ContentHandler;
use tracing::{debug, error};

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
    action_handler!(|instance| {
        debug!("📂 Browse handler called");

        let handler = ContentHandler::new();

        // Extraire les arguments d'entrée
        let object_id = match instance
            .argument("ObjectID")
            .and_then(|arg| arg.get_variable_instance())
            .ok_or_else(|| ActionError::ArgumentError("ObjectID not found".to_string()))?
            .value()
        {
            StateValue::String(s) => s,
            _ => return Err(ActionError::ArgumentError("ObjectID must be a string".to_string())),
        };

        let browse_flag = match instance
            .argument("BrowseFlag")
            .and_then(|arg| arg.get_variable_instance())
            .ok_or_else(|| ActionError::ArgumentError("BrowseFlag not found".to_string()))?
            .value()
        {
            StateValue::String(s) => s,
            _ => return Err(ActionError::ArgumentError("BrowseFlag must be a string".to_string())),
        };

        let starting_index = match instance
            .argument("StartingIndex")
            .and_then(|arg| arg.get_variable_instance())
            .ok_or_else(|| ActionError::ArgumentError("StartingIndex not found".to_string()))?
            .value()
        {
            StateValue::UI4(n) => n,
            _ => return Err(ActionError::ArgumentError("StartingIndex must be ui4".to_string())),
        };

        let requested_count = match instance
            .argument("RequestedCount")
            .and_then(|arg| arg.get_variable_instance())
            .ok_or_else(|| ActionError::ArgumentError("RequestedCount not found".to_string()))?
            .value()
        {
            StateValue::UI4(n) => n,
            _ => return Err(ActionError::ArgumentError("RequestedCount must be ui4".to_string())),
        };

        // Appeler la logique métier
        let (didl, returned, total, update_id) = handler
            .browse(&object_id, &browse_flag, starting_index, requested_count)
            .await
            .map_err(|e| {
                error!("Browse failed: {}", e);
                ActionError::GeneralError(e)
            })?;

        // Définir les arguments de sortie
        if let Some(arg) = instance.argument("Result") {
            if let Some(var) = arg.get_variable_instance() {
                var.set_value(StateValue::String(didl)).await;
            }
        }

        if let Some(arg) = instance.argument("NumberReturned") {
            if let Some(var) = arg.get_variable_instance() {
                var.set_value(StateValue::UI4(returned)).await;
            }
        }

        if let Some(arg) = instance.argument("TotalMatches") {
            if let Some(var) = arg.get_variable_instance() {
                var.set_value(StateValue::UI4(total)).await;
            }
        }

        if let Some(arg) = instance.argument("UpdateID") {
            if let Some(var) = arg.get_variable_instance() {
                var.set_value(StateValue::UI4(update_id)).await;
            }
        }

        debug!("✅ Browse completed: returned={}, total={}", returned, total);
        Ok(())
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
    action_handler!(|instance| {
        debug!("🔍 Search handler called");

        let handler = ContentHandler::new();

        let container_id = match instance
            .argument("ContainerID")
            .and_then(|arg| arg.get_variable_instance())
            .ok_or_else(|| ActionError::ArgumentError("ContainerID not found".to_string()))?
            .value()
        {
            StateValue::String(s) => s,
            _ => return Err(ActionError::ArgumentError("ContainerID must be a string".to_string())),
        };

        let search_criteria = match instance
            .argument("SearchCriteria")
            .and_then(|arg| arg.get_variable_instance())
            .ok_or_else(|| ActionError::ArgumentError("SearchCriteria not found".to_string()))?
            .value()
        {
            StateValue::String(s) => s,
            _ => return Err(ActionError::ArgumentError("SearchCriteria must be a string".to_string())),
        };

        let (didl, returned, total, update_id) = handler
            .search(&container_id, &search_criteria)
            .await
            .map_err(|e| {
                error!("Search failed: {}", e);
                ActionError::GeneralError(e)
            })?;

        // Définir les sorties
        if let Some(arg) = instance.argument("Result") {
            if let Some(var) = arg.get_variable_instance() {
                var.set_value(StateValue::String(didl)).await;
            }
        }

        if let Some(arg) = instance.argument("NumberReturned") {
            if let Some(var) = arg.get_variable_instance() {
                var.set_value(StateValue::UI4(returned)).await;
            }
        }

        if let Some(arg) = instance.argument("TotalMatches") {
            if let Some(var) = arg.get_variable_instance() {
                var.set_value(StateValue::UI4(total)).await;
            }
        }

        if let Some(arg) = instance.argument("UpdateID") {
            if let Some(var) = arg.get_variable_instance() {
                var.set_value(StateValue::UI4(update_id)).await;
            }
        }

        debug!("✅ Search completed: returned={}, total={}", returned, total);
        Ok(())
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
    action_handler!(|instance| {
        debug!("🔍 GetSearchCapabilities handler called");

        let handler = ContentHandler::new();
        let capabilities = handler.get_search_capabilities().await;

        if let Some(arg) = instance.argument("SearchCaps") {
            if let Some(var) = arg.get_variable_instance() {
                var.set_value(StateValue::String(capabilities.clone())).await;
            }
        }

        debug!("✅ SearchCapabilities: {}", capabilities);
        Ok(())
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
    action_handler!(|instance| {
        debug!("📊 GetSortCapabilities handler called");

        let handler = ContentHandler::new();
        let capabilities = handler.get_sort_capabilities().await;

        if let Some(arg) = instance.argument("SortCaps") {
            if let Some(var) = arg.get_variable_instance() {
                var.set_value(StateValue::String(capabilities.clone())).await;
            }
        }

        debug!("✅ SortCapabilities: {}", capabilities);
        Ok(())
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
    action_handler!(|instance| {
        debug!("🔄 GetSystemUpdateID handler called");

        let handler = ContentHandler::new();
        let update_id = handler.get_system_update_id().await;

        if let Some(arg) = instance.argument("Id") {
            if let Some(var) = arg.get_variable_instance() {
                var.set_value(StateValue::UI4(update_id)).await;
            }
        }

        debug!("✅ SystemUpdateID: {}", update_id);
        Ok(())
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
