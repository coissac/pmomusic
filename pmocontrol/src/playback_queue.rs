use crate::media_server::ServerId;

#[derive(Clone, Debug)]
pub struct PlaybackItem {
    pub uri: String,
    pub title: Option<String>,
    pub server_id: Option<ServerId>,
    pub object_id: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    pub album_art_uri: Option<String>,
    pub date: Option<String>,
    pub track_number: Option<String>,
    pub creator: Option<String>,
    pub protocol_info: Option<String>,
}

impl PlaybackItem {
    pub fn new(uri: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            title: None,
            server_id: None,
            object_id: None,
            artist: None,
            album: None,
            genre: None,
            album_art_uri: None,
            date: None,
            track_number: None,
            creator: None,
            protocol_info: None,
        }
    }

    /// Convert PlaybackItem to DIDL-Lite XML metadata for SetAVTransportURI
    pub fn to_didl_metadata(&self) -> String {
        use quick_xml::escape::escape;
        use tracing::debug;

        let title = self.title.as_deref().unwrap_or("Unknown");
        let escaped_uri = escape(&self.uri);
        let escaped_title = escape(title);

        let mut didl = String::from(
            r#"<DIDL-Lite xmlns="urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:upnp="urn:schemas-upnp-org:metadata-1-0/upnp/">"#,
        );
        // Use proper ID from object_id if available, otherwise use "0"
        let item_id = self.object_id.as_deref().unwrap_or("0");
        let escaped_item_id = escape(item_id);
        didl.push_str(&format!(
            r#"<item id="{}" parentID="-1" restricted="1">"#,
            escaped_item_id
        ));
        didl.push_str(&format!("<dc:title>{}</dc:title>", escaped_title));

        if let Some(artist) = &self.artist {
            let escaped_artist = escape(artist);
            didl.push_str(&format!("<upnp:artist>{}</upnp:artist>", escaped_artist));
            didl.push_str(&format!("<dc:creator>{}</dc:creator>", escaped_artist));
        }

        if let Some(album) = &self.album {
            let escaped_album = escape(album);
            didl.push_str(&format!("<upnp:album>{}</upnp:album>", escaped_album));
        }

        if let Some(genre) = &self.genre {
            let escaped_genre = escape(genre);
            didl.push_str(&format!("<upnp:genre>{}</upnp:genre>", escaped_genre));
        }

        if let Some(album_art) = &self.album_art_uri {
            let escaped_art = escape(album_art);
            debug!(
                title = title,
                album_art_uri = album_art.as_str(),
                "Including albumArtURI in DIDL metadata"
            );
            didl.push_str(&format!(
                "<upnp:albumArtURI>{}</upnp:albumArtURI>",
                escaped_art
            ));
        } else {
            debug!(
                title = title,
                "No album_art_uri in PlaybackItem - skipping albumArtURI in DIDL"
            );
        }

        if let Some(date) = &self.date {
            let escaped_date = escape(date);
            didl.push_str(&format!("<dc:date>{}</dc:date>", escaped_date));
        }

        if let Some(track_num) = &self.track_number {
            let escaped_track = escape(track_num);
            didl.push_str(&format!(
                "<upnp:originalTrackNumber>{}</upnp:originalTrackNumber>",
                escaped_track
            ));
        }

        // Add resource with URI
        // Use the original protocolInfo if available, otherwise use a generic one
        let protocol_info = self
            .protocol_info
            .as_deref()
            .unwrap_or("http-get:*:audio/*:*");

        // For protocolInfo, we only need to escape XML special chars, not ':'
        // We manually escape only the necessary characters to preserve the protocolInfo format
        let safe_protocol_info = protocol_info
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;");

        didl.push_str(&format!(
            r#"<res protocolInfo="{}">{}</res>"#,
            safe_protocol_info, escaped_uri
        ));

        didl.push_str(r#"<upnp:class>object.item.audioItem.musicTrack</upnp:class>"#);
        didl.push_str("</item>");
        didl.push_str("</DIDL-Lite>");

        didl
    }
}

#[derive(Clone, Debug, Default)]
pub struct PlaybackQueue {
    items: Vec<PlaybackItem>,
    current_index: Option<usize>,
}

impl PlaybackQueue {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            current_index: None,
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.current_index = None;
    }

    pub fn enqueue(&mut self, item: PlaybackItem) {
        self.items.push(item);
    }

    pub fn enqueue_many<I: IntoIterator<Item = PlaybackItem>>(&mut self, items: I) {
        for item in items {
            self.items.push(item);
        }
    }

    pub fn enqueue_front(&mut self, item: PlaybackItem) {
        let insert_at = match self.current_index {
            Some(idx) => {
                let next = idx.saturating_add(1);
                next.min(self.items.len())
            }
            None => 0,
        };
        self.items.insert(insert_at, item);
        // current_index remains unchanged; insertion happens after the cursor.
    }

    pub fn dequeue(&mut self) -> Option<PlaybackItem> {
        if self.items.is_empty() {
            return None;
        }

        match self.current_index {
            None => {
                self.current_index = Some(0);
                self.items.get(0).cloned()
            }
            Some(idx) => {
                let next_idx = idx + 1;
                if next_idx >= self.items.len() {
                    None
                } else {
                    self.current_index = Some(next_idx);
                    self.items.get(next_idx).cloned()
                }
            }
        }
    }

    pub fn peek(&self) -> Option<&PlaybackItem> {
        if let Some(idx) = self.current_index {
            self.items.get(idx)
        } else {
            self.items.first()
        }
    }

    pub fn snapshot(&self) -> Vec<PlaybackItem> {
        match self.current_index {
            None => self.items.clone(),
            Some(idx) => self.items.iter().skip(idx + 1).cloned().collect(),
        }
    }

    pub fn upcoming_len(&self) -> usize {
        match self.current_index {
            None => self.items.len(),
            Some(idx) => self.items.len().saturating_sub(idx + 1),
        }
    }

    pub fn full_snapshot(&self) -> (Vec<PlaybackItem>, Option<usize>) {
        (self.items.clone(), self.current_index)
    }

    pub fn set_current_index(&mut self, index: Option<usize>) {
        if let Some(idx) = index {
            if idx < self.items.len() {
                self.current_index = Some(idx);
            } else {
                self.current_index = None;
            }
        } else {
            self.current_index = None;
        }
    }
}
