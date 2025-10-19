//! # ContentDirectory Handler - Gestionnaire du service ContentDirectory
//!
//! Ce module implémente la logique métier du service ContentDirectory en intégrant
//! les sources musicales enregistrées dans le registre.
//!
//! ## Fonctionnalités
//!
//! - **Navigation multi-sources** : Combine toutes les sources dans une hiérarchie
//! - **Browse** : Parcours des containers et items
//! - **Search** : Recherche dans les sources qui le supportent
//! - **Update ID** : Suivi des changements pour les notifications UPnP

use pmodidl::{Container, DIDLLite};
use pmosource::api::{get_source as get_source_from_registry, list_all_sources};
use pmosource::{BrowseResult, MusicSource};
use std::sync::Arc;

/// Convertit des containers et items en XML DIDL-Lite
fn to_didl_lite(containers: &[Container], items: &[pmodidl::Item]) -> Result<String, String> {
    let didl = DIDLLite {
        xmlns: "urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/".to_string(),
        xmlns_upnp: Some("urn:schemas-upnp-org:metadata-1-0/upnp/".to_string()),
        xmlns_dc: Some("http://purl.org/dc/elements/1.1/".to_string()),
        xmlns_dlna: Some("urn:schemas-dlna-org:metadata-1-0/".to_string()),
        xmlns_pv: None,
        xmlns_sec: None,
        containers: containers.to_vec(),
        items: items.to_vec(),
    };

    quick_xml::se::to_string(&didl).map_err(|e| format!("Failed to serialize DIDL-Lite: {}", e))
}

/// Handler pour le service ContentDirectory
///
/// Ce handler gère toutes les opérations du ContentDirectory en utilisant
/// les sources musicales enregistrées dans le registre global.
pub struct ContentHandler;

impl ContentHandler {
    /// Crée un nouveau ContentHandler
    pub fn new() -> Self {
        Self
    }

    /// Browse un container ou récupère les métadonnées d'un objet
    ///
    /// # Arguments
    ///
    /// * `object_id` - L'ID de l'objet à parcourir ("0" pour la racine)
    /// * `browse_flag` - "BrowseMetadata" ou "BrowseDirectChildren"
    /// * `starting_index` - Index de départ pour la pagination
    /// * `requested_count` - Nombre d'éléments demandés (0 = tous)
    ///
    /// # Returns
    ///
    /// Un tuple contenant:
    /// - Le résultat DIDL-Lite XML
    /// - Le nombre d'éléments retournés
    /// - Le nombre total d'éléments
    /// - L'update ID
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let handler = ContentHandler::new();
    /// let (didl, returned, total, update_id) =
    ///     handler.browse("0", "BrowseDirectChildren", 0, 0).await?;
    /// ```
    pub async fn browse(
        &self,
        object_id: &str,
        browse_flag: &str,
        starting_index: u32,
        requested_count: u32,
    ) -> Result<(String, u32, u32, u32), String> {
        tracing::debug!(
            object_id = %object_id,
            browse_flag = %browse_flag,
            starting_index = %starting_index,
            requested_count = %requested_count,
            "ContentDirectory::Browse"
        );

        match browse_flag {
            "BrowseMetadata" => self.browse_metadata(object_id).await,
            "BrowseDirectChildren" => {
                self.browse_direct_children(object_id, starting_index, requested_count)
                    .await
            }
            _ => Err(format!("Invalid BrowseFlag: {}", browse_flag)),
        }
    }

    /// Browse les métadonnées d'un objet spécifique
    async fn browse_metadata(&self, object_id: &str) -> Result<(String, u32, u32, u32), String> {
        if object_id == "0" {
            // Retourner le container racine
            let root = self.build_root_container().await;
            let didl = to_didl_lite(&[root], &[])?;
            Ok((didl, 1, 1, 0))
        } else {
            // Essayer de trouver l'objet dans les sources
            // Vérifier si c'est un container racine d'une source
            if let Some(source) = get_source_from_registry(object_id).await {
                let container = source
                    .root_container()
                    .await
                    .map_err(|e| format!("Failed to get root container: {}", e))?;
                let didl = to_didl_lite(&[container], &[])?;
                let update_id = source.update_id().await;
                return Ok((didl, 1, 1, update_id));
            }

            // Sinon, chercher dans les sources
            for source in list_all_sources().await {
                if let Ok(result) = source.browse(object_id).await {
                    // L'objet a été trouvé, retourner ses métadonnées
                    match result {
                        BrowseResult::Containers(containers) => {
                            if let Some(container) = containers.first() {
                                let didl = to_didl_lite(&[container.clone()], &[])?;
                                let update_id = source.update_id().await;
                                return Ok((didl, 1, 1, update_id));
                            }
                        }
                        BrowseResult::Items(items) => {
                            if let Some(item) = items.first() {
                                let didl = to_didl_lite(&[], &[item.clone()])?;
                                let update_id = source.update_id().await;
                                return Ok((didl, 1, 1, update_id));
                            }
                        }
                        BrowseResult::Mixed { containers, items } => {
                            if let Some(container) = containers.first() {
                                let didl = to_didl_lite(&[container.clone()], &[])?;
                                let update_id = source.update_id().await;
                                return Ok((didl, 1, 1, update_id));
                            } else if let Some(item) = items.first() {
                                let didl = to_didl_lite(&[], &[item.clone()])?;
                                let update_id = source.update_id().await;
                                return Ok((didl, 1, 1, update_id));
                            }
                        }
                    }
                }
            }

            Err(format!("Object not found: {}", object_id))
        }
    }

    /// Browse les enfants directs d'un container
    async fn browse_direct_children(
        &self,
        object_id: &str,
        starting_index: u32,
        requested_count: u32,
    ) -> Result<(String, u32, u32, u32), String> {
        if object_id == "0" {
            // Retourner toutes les sources comme enfants de la racine
            return self.browse_root(starting_index, requested_count).await;
        }

        // Vérifier si c'est le container racine d'une source
        if let Some(source) = get_source_from_registry(object_id).await {
            return self
                .browse_source_root(source, starting_index, requested_count)
                .await;
        }

        // Sinon, chercher dans les sources
        for source in list_all_sources().await {
            if let Ok(result) = source.browse(object_id).await {
                return self
                    .browse_result_to_didl(result, source, starting_index, requested_count)
                    .await;
            }
        }

        Err(format!("Container not found: {}", object_id))
    }

    /// Browse la racine (liste toutes les sources)
    async fn browse_root(
        &self,
        starting_index: u32,
        requested_count: u32,
    ) -> Result<(String, u32, u32, u32), String> {
        let sources = list_all_sources().await;

        let mut containers = Vec::new();
        for source in sources.iter() {
            let container = source
                .root_container()
                .await
                .map_err(|e| format!("Failed to get root container: {}", e))?;
            containers.push(container);
        }

        // Appliquer la pagination
        let total = containers.len();
        let start = starting_index as usize;
        let count = if requested_count == 0 {
            total - start
        } else {
            requested_count as usize
        };

        let paginated: Vec<Container> = containers.into_iter().skip(start).take(count).collect();

        let returned = paginated.len();
        let didl = to_didl_lite(&paginated, &[])?;

        Ok((didl, returned as u32, total as u32, 0))
    }

    /// Browse le container racine d'une source spécifique
    async fn browse_source_root(
        &self,
        source: Arc<dyn MusicSource>,
        starting_index: u32,
        requested_count: u32,
    ) -> Result<(String, u32, u32, u32), String> {
        let result = source
            .browse(source.id())
            .await
            .map_err(|e| format!("Browse failed: {}", e))?;

        self.browse_result_to_didl(result, source, starting_index, requested_count)
            .await
    }

    /// Convertit un BrowseResult en DIDL-Lite XML avec pagination
    async fn browse_result_to_didl(
        &self,
        result: BrowseResult,
        source: Arc<dyn MusicSource>,
        starting_index: u32,
        requested_count: u32,
    ) -> Result<(String, u32, u32, u32), String> {
        let (mut containers, mut items) = match result {
            BrowseResult::Containers(c) => (c, vec![]),
            BrowseResult::Items(i) => (vec![], i),
            BrowseResult::Mixed { containers, items } => (containers, items),
        };

        // Calculer le total avant pagination
        let total = (containers.len() + items.len()) as u32;

        // Appliquer la pagination
        let start = starting_index as usize;
        let count = if requested_count == 0 {
            total as usize - start
        } else {
            requested_count as usize
        };

        // Pagination sur les containers d'abord, puis les items
        let total_containers = containers.len();
        if start < total_containers {
            // On commence dans les containers
            containers = containers.into_iter().skip(start).collect();
            let remaining = count.saturating_sub(containers.len());
            containers.truncate(count);

            if remaining > 0 && !items.is_empty() {
                items.truncate(remaining);
            } else {
                items.clear();
            }
        } else {
            // On commence dans les items
            containers.clear();
            let item_start = start - total_containers;
            items = items.into_iter().skip(item_start).take(count).collect();
        }

        let returned = (containers.len() + items.len()) as u32;
        let didl = to_didl_lite(&containers, &items)?;
        let update_id = source.update_id().await;

        Ok((didl, returned, total, update_id))
    }

    /// Construit le container racine du MediaServer
    async fn build_root_container(&self) -> Container {
        let sources = list_all_sources().await;
        let child_count = sources.len();

        Container {
            id: "0".to_string(),
            parent_id: "-1".to_string(),
            restricted: Some("1".to_string()),
            child_count: Some(child_count.to_string()),
            title: "PMOMusic".to_string(),
            class: "object.container".to_string(),
            containers: vec![],
            items: vec![],
        }
    }

    /// Recherche dans toutes les sources qui supportent la recherche
    ///
    /// # Arguments
    ///
    /// * `container_id` - ID du container dans lequel rechercher ("0" = partout)
    /// * `search_criteria` - Critères de recherche UPnP
    ///
    /// # Returns
    ///
    /// Les mêmes informations que browse()
    pub async fn search(
        &self,
        container_id: &str,
        search_criteria: &str,
    ) -> Result<(String, u32, u32, u32), String> {
        tracing::debug!(
            container_id = %container_id,
            search_criteria = %search_criteria,
            "ContentDirectory::Search"
        );

        let mut all_containers = Vec::new();
        let mut all_items = Vec::new();

        // Rechercher dans toutes les sources qui supportent la recherche
        for source in list_all_sources().await {
            if source.capabilities().supports_search {
                if let Ok(result) = source.search(search_criteria).await {
                    match result {
                        BrowseResult::Containers(c) => all_containers.extend(c),
                        BrowseResult::Items(i) => all_items.extend(i),
                        BrowseResult::Mixed { containers, items } => {
                            all_containers.extend(containers);
                            all_items.extend(items);
                        }
                    }
                }
            }
        }

        let total = (all_containers.len() + all_items.len()) as u32;
        let didl = to_didl_lite(&all_containers, &all_items)?;

        Ok((didl, total, total, 0))
    }

    /// Retourne les capacités de recherche
    pub async fn get_search_capabilities(&self) -> String {
        // Capacités de recherche de base UPnP
        "dc:title,dc:creator,upnp:artist,upnp:album,upnp:genre".to_string()
    }

    /// Retourne les capacités de tri
    pub async fn get_sort_capabilities(&self) -> String {
        // Capacités de tri de base UPnP
        "dc:title,dc:date,upnp:artist,upnp:album".to_string()
    }

    /// Retourne le system update ID global
    pub async fn get_system_update_id(&self) -> u32 {
        let sources = list_all_sources().await;

        // Combiner les update IDs de toutes les sources
        let mut combined_id = 0u32;
        for source in sources {
            combined_id = combined_id.wrapping_add(source.update_id().await);
        }

        combined_id
    }
}

impl Default for ContentHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_content_handler_creation() {
        let handler = ContentHandler::new();
        let capabilities = handler.get_search_capabilities().await;
        assert!(capabilities.contains("dc:title"));
    }

    #[tokio::test]
    async fn test_browse_root_empty() {
        let handler = ContentHandler::new();
        let result = handler.browse("0", "BrowseDirectChildren", 0, 0).await;
        assert!(result.is_ok());

        let (didl, returned, total, _) = result.unwrap();
        assert_eq!(returned, 0);
        assert_eq!(total, 0);
        assert!(didl.contains("DIDL-Lite"));
    }

    #[tokio::test]
    async fn test_get_system_update_id() {
        let handler = ContentHandler::new();
        let update_id = handler.get_system_update_id().await;
        assert_eq!(update_id, 0); // No sources registered
    }
}
