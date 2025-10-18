//! ContentDirectory service implementation

use crate::RadioParadiseClient;
use pmoupnp::services::Service;
use pmoupnp::actions::Action;
use pmoupnp::state_variables::StateVariable;
use pmodidl::{DIDLObject, DIDLContainer, DIDLItem, Resource};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Create a ContentDirectory service for Radio Paradise
///
/// The ContentDirectory service allows browsing Radio Paradise blocks and songs.
pub fn create_content_directory_service(
    client: Arc<RwLock<RadioParadiseClient>>,
) -> Service {
    let mut service = Service::new("ContentDirectory".to_string());
    service.set_service_type("urn:schemas-upnp-org:service:ContentDirectory:1".to_string());
    service.set_service_id("urn:upnp-org:serviceId:ContentDirectory".to_string());

    // State variables
    let system_update_id = StateVariable::new(
        "SystemUpdateID".to_string(),
        "ui4".to_string(),
    ).with_send_events(true)
     .with_default_value("0".to_string());

    let container_update_ids = StateVariable::new(
        "ContainerUpdateIDs".to_string(),
        "string".to_string(),
    ).with_send_events(true)
     .with_default_value("".to_string());

    service.add_state_variable(Arc::new(system_update_id));
    service.add_state_variable(Arc::new(container_update_ids));

    // Browse action
    let mut browse = Action::new("Browse".to_string());
    browse.add_input_argument("ObjectID".to_string(), "A_ARG_TYPE_ObjectID".to_string());
    browse.add_input_argument("BrowseFlag".to_string(), "A_ARG_TYPE_BrowseFlag".to_string());
    browse.add_input_argument("Filter".to_string(), "A_ARG_TYPE_Filter".to_string());
    browse.add_input_argument("StartingIndex".to_string(), "A_ARG_TYPE_Index".to_string());
    browse.add_input_argument("RequestedCount".to_string(), "A_ARG_TYPE_Count".to_string());
    browse.add_input_argument("SortCriteria".to_string(), "A_ARG_TYPE_SortCriteria".to_string());
    browse.add_output_argument("Result".to_string(), "A_ARG_TYPE_Result".to_string());
    browse.add_output_argument("NumberReturned".to_string(), "A_ARG_TYPE_Count".to_string());
    browse.add_output_argument("TotalMatches".to_string(), "A_ARG_TYPE_Count".to_string());
    browse.add_output_argument("UpdateID".to_string(), "A_ARG_TYPE_UpdateID".to_string());

    // Store client reference for the action handler
    let client_clone = client.clone();
    browse.set_handler(Box::new(move |args| {
        let client = client_clone.clone();
        Box::pin(async move {
            handle_browse(client, args).await
        })
    }));

    service.add_action(Arc::new(browse));

    // GetSearchCapabilities action
    let mut get_search_caps = Action::new("GetSearchCapabilities".to_string());
    get_search_caps.add_output_argument(
        "SearchCaps".to_string(),
        "A_ARG_TYPE_SearchCaps".to_string(),
    );
    get_search_caps.set_handler(Box::new(|_| {
        Box::pin(async {
            let mut result = std::collections::HashMap::new();
            result.insert("SearchCaps".to_string(), "".to_string());
            Ok(result)
        })
    }));
    service.add_action(Arc::new(get_search_caps));

    // GetSortCapabilities action
    let mut get_sort_caps = Action::new("GetSortCapabilities".to_string());
    get_sort_caps.add_output_argument(
        "SortCaps".to_string(),
        "A_ARG_TYPE_SortCaps".to_string(),
    );
    get_sort_caps.set_handler(Box::new(|_| {
        Box::pin(async {
            let mut result = std::collections::HashMap::new();
            result.insert("SortCaps".to_string(), "dc:title".to_string());
            Ok(result)
        })
    }));
    service.add_action(Arc::new(get_sort_caps));

    // GetSystemUpdateID action
    let mut get_update_id = Action::new("GetSystemUpdateID".to_string());
    get_update_id.add_output_argument("Id".to_string(), "SystemUpdateID".to_string());
    get_update_id.set_handler(Box::new(|_| {
        Box::pin(async {
            let mut result = std::collections::HashMap::new();
            result.insert("Id".to_string(), "0".to_string());
            Ok(result)
        })
    }));
    service.add_action(Arc::new(get_update_id));

    // Argument state variables
    service.add_state_variable(Arc::new(
        StateVariable::new("A_ARG_TYPE_ObjectID".to_string(), "string".to_string())
    ));
    service.add_state_variable(Arc::new(
        StateVariable::new("A_ARG_TYPE_BrowseFlag".to_string(), "string".to_string())
            .with_allowed_values(vec![
                "BrowseMetadata".to_string(),
                "BrowseDirectChildren".to_string(),
            ])
    ));
    service.add_state_variable(Arc::new(
        StateVariable::new("A_ARG_TYPE_Filter".to_string(), "string".to_string())
    ));
    service.add_state_variable(Arc::new(
        StateVariable::new("A_ARG_TYPE_Index".to_string(), "ui4".to_string())
    ));
    service.add_state_variable(Arc::new(
        StateVariable::new("A_ARG_TYPE_Count".to_string(), "ui4".to_string())
    ));
    service.add_state_variable(Arc::new(
        StateVariable::new("A_ARG_TYPE_SortCriteria".to_string(), "string".to_string())
    ));
    service.add_state_variable(Arc::new(
        StateVariable::new("A_ARG_TYPE_Result".to_string(), "string".to_string())
    ));
    service.add_state_variable(Arc::new(
        StateVariable::new("A_ARG_TYPE_UpdateID".to_string(), "ui4".to_string())
    ));
    service.add_state_variable(Arc::new(
        StateVariable::new("A_ARG_TYPE_SearchCaps".to_string(), "string".to_string())
    ));
    service.add_state_variable(Arc::new(
        StateVariable::new("A_ARG_TYPE_SortCaps".to_string(), "string".to_string())
    ));

    service
}

/// Handle Browse action
async fn handle_browse(
    client: Arc<RwLock<RadioParadiseClient>>,
    args: std::collections::HashMap<String, String>,
) -> Result<std::collections::HashMap<String, String>, String> {
    let object_id = args.get("ObjectID").ok_or("Missing ObjectID")?;
    let browse_flag = args.get("BrowseFlag").ok_or("Missing BrowseFlag")?;
    let starting_index: usize = args.get("StartingIndex")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let requested_count: usize = args.get("RequestedCount")
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);

    let client = client.read().await;

    let (didl_result, number_returned, total_matches) = match object_id.as_str() {
        "0" => {
            // Root container - show current block
            if browse_flag == "BrowseMetadata" {
                let root = create_root_container();
                (serialize_didl(&[root]), 1, 1)
            } else {
                // BrowseDirectChildren - show current block as a container
                let block = client.get_block(None).await
                    .map_err(|e| format!("Failed to get block: {}", e))?;

                let block_container = create_block_container(&block);
                (serialize_didl(&[block_container]), 1, 1)
            }
        }
        id if id.starts_with("block:") => {
            // Browse songs in a block
            let event_id: u64 = id.strip_prefix("block:")
                .and_then(|s| s.parse().ok())
                .ok_or("Invalid block ID")?;

            let block = client.get_block(Some(event_id)).await
                .map_err(|e| format!("Failed to get block: {}", e))?;

            if browse_flag == "BrowseMetadata" {
                let container = create_block_container(&block);
                (serialize_didl(&[container]), 1, 1)
            } else {
                // BrowseDirectChildren - show songs
                let songs = block.songs_ordered();
                let total = songs.len();
                let songs_slice = songs.iter()
                    .skip(starting_index)
                    .take(requested_count)
                    .collect::<Vec<_>>();

                let items: Vec<DIDLObject> = songs_slice.iter()
                    .map(|(idx, song)| create_song_item(&block, *idx, song))
                    .collect();

                (serialize_didl(&items), items.len(), total)
            }
        }
        _ => {
            return Err(format!("Unknown ObjectID: {}", object_id));
        }
    };

    let mut result = std::collections::HashMap::new();
    result.insert("Result".to_string(), didl_result);
    result.insert("NumberReturned".to_string(), number_returned.to_string());
    result.insert("TotalMatches".to_string(), total_matches.to_string());
    result.insert("UpdateID".to_string(), "0".to_string());

    Ok(result)
}

/// Create the root container
fn create_root_container() -> DIDLObject {
    let mut container = DIDLContainer::new("0".to_string(), "-1".to_string());
    container.set_title("Radio Paradise".to_string());
    container.set_class("object.container.storageFolder".to_string());
    container.set_searchable(false);
    container.set_child_count(Some(1));
    DIDLObject::Container(container)
}

/// Create a container for a block
fn create_block_container(block: &crate::models::Block) -> DIDLObject {
    let mut container = DIDLContainer::new(
        format!("block:{}", block.event),
        "0".to_string(),
    );
    container.set_title(format!("Block {} ({} songs)", block.event, block.song_count()));
    container.set_class("object.container.album.musicAlbum".to_string());
    container.set_searchable(false);
    container.set_child_count(Some(block.song_count()));

    // Add album art if available
    if let Some(first_song) = block.get_song(0) {
        if let Some(cover) = &first_song.cover {
            if let Some(cover_url) = block.cover_url(cover) {
                container.add_album_art_uri(cover_url);
            }
        }
    }

    DIDLObject::Container(container)
}

/// Create an item for a song
fn create_song_item(
    block: &crate::models::Block,
    index: usize,
    song: &crate::models::Song,
) -> DIDLObject {
    let mut item = DIDLItem::new(
        format!("block:{}:song:{}", block.event, index),
        format!("block:{}", block.event),
    );

    item.set_title(song.title.clone());
    item.set_class("object.item.audioItem.musicTrack".to_string());

    // Add metadata
    item.add_artist(song.artist.clone());
    if let Some(ref album) = song.album {
        item.add_album(album.clone());
    }

    if let Some(year) = song.year {
        item.set_date(format!("{}-01-01", year));
    }

    // Add album art
    if let Some(cover) = &song.cover {
        if let Some(cover_url) = block.cover_url(cover) {
            item.add_album_art_uri(cover_url);
        }
    }

    // Add resource for streaming
    let mut resource = Resource::new(block.url.clone());
    resource.set_protocol_info("http-get:*:audio/flac:*".to_string());
    resource.set_duration(format_duration(song.duration));
    resource.set_size(None); // Unknown size

    item.add_resource(resource);

    DIDLObject::Item(item)
}

/// Format duration in H:MM:SS format
fn format_duration(duration_ms: u64) -> String {
    let total_seconds = duration_ms / 1000;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    format!("{}:{:02}:{:02}", hours, minutes, seconds)
}

/// Serialize DIDL objects to XML string
fn serialize_didl(objects: &[DIDLObject]) -> String {
    let mut didl = String::from(r#"<DIDL-Lite xmlns="urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:upnp="urn:schemas-upnp-org:metadata-1-0/upnp/">"#);

    for obj in objects {
        didl.push_str(&obj.to_didl());
    }

    didl.push_str("</DIDL-Lite>");
    didl
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(0), "0:00:00");
        assert_eq!(format_duration(60000), "0:01:00");
        assert_eq!(format_duration(3661000), "1:01:01");
    }

    #[test]
    fn test_create_root_container() {
        let root = create_root_container();
        if let DIDLObject::Container(container) = root {
            assert_eq!(container.id(), "0");
            assert_eq!(container.parent_id(), "-1");
        } else {
            panic!("Expected Container");
        }
    }
}
