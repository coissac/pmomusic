use std::time::Duration;

use serde::Deserialize;
use ureq::Agent;

use crate::{
    errors::ControlPointError,
    model::PlaybackState,
    music_renderer::{
        PlaybackPositionInfo,
        time_utils::{format_hhmmss, ms_to_seconds},
    },
};

const STATUS_COMMAND: &str = "getPlayerStatus";

/// Raw response from LinkPlay getPlayerStatus API
#[derive(Debug, Deserialize)]
struct LinkPlayStatusRaw {
    status: String,
    curpos: String,
    totlen: String,
    vol: String,
    mute: String,
    #[serde(default)]
    plicurr: Option<String>,
}

#[derive(Clone, Debug)]
pub struct LinkPlayStatus {
    state_raw: String,
    pub curpos_ms: u64,
    pub totlen_ms: u64,
    pub track_index: Option<u32>,
    pub volume: u16,
    pub mute: bool,
}

impl LinkPlayStatus {
    pub fn playback_state(&self) -> PlaybackState {
        match self.state_raw.as_str() {
            "play" => PlaybackState::Playing,
            "pause" => PlaybackState::Paused,
            "stop" => PlaybackState::Stopped,
            "load" => PlaybackState::Transitioning,
            other => PlaybackState::Unknown(other.to_string()),
        }
    }

    pub fn position_info(&self) -> PlaybackPositionInfo {
        PlaybackPositionInfo {
            track: self.track_index,
            rel_time: Some(format_hhmmss(ms_to_seconds(self.curpos_ms))),
            abs_time: None,
            track_duration: if self.totlen_ms > 0 {
                Some(format_hhmmss(ms_to_seconds(self.totlen_ms)))
            } else {
                None
            },
            track_metadata: None,
            track_uri: None,
        }
    }
}

/// Extract the IP/host component from a LOCATION URL.
pub fn extract_linkplay_host(location: &str) -> Option<String> {
    let (_, rest) = location.split_once("://")?;
    let authority = rest.split('/').next().unwrap_or(rest);
    let without_auth = authority.split('@').last().unwrap_or(authority);

    if without_auth.starts_with('[') {
        let end = without_auth.find(']')?;
        let host = &without_auth[1..end];
        if host.is_empty() {
            None
        } else {
            Some(host.to_string())
        }
    } else {
        let host = without_auth.split(':').next().unwrap_or("");
        if host.is_empty() {
            None
        } else {
            Some(host.to_string())
        }
    }
}

pub fn build_agent(timeout: Duration) -> Agent {
    Agent::config_builder()
        .timeout_global(Some(timeout))
        .build()
        .into()
}

pub fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for b in input.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

pub fn fetch_status_for_host(
    host: &str,
    timeout: Duration,
) -> Result<LinkPlayStatus, ControlPointError> {
    let url = format!("http://{}/httpapi.asp?command={}", host, STATUS_COMMAND);
    let mut response = build_agent(timeout).get(&url).call().map_err(|_| {
        ControlPointError::ArilycTcpError(format!(
            "HTTP request failed for LinkPlay status on {}",
            host
        ))
    })?;

    let body = response.body_mut().read_to_string().map_err(|e| {
        ControlPointError::ArilycTcpError(format!("Failed to read LinkPlay status body : {}", e))
    })?;

    parse_linkplay_status(&body)
}

fn parse_linkplay_status(body: &str) -> Result<LinkPlayStatus, ControlPointError> {
    let raw: LinkPlayStatusRaw = serde_json::from_str(body).map_err(|e| {
        ControlPointError::LinkPlayError(format!("Failed to parse LinkPlay status JSON: {}", e))
    })?;

    let curpos_ms = raw.curpos.parse::<u64>().map_err(|_| {
        ControlPointError::LinkPlayError(format!("Invalid curpos value: {}", raw.curpos))
    })?;

    let totlen_ms = raw.totlen.parse::<u64>().map_err(|_| {
        ControlPointError::LinkPlayError(format!("Invalid totlen value: {}", raw.totlen))
    })?;

    let volume = raw
        .vol
        .parse::<u16>()
        .map_err(|_| ControlPointError::LinkPlayError(format!("Invalid vol value: {}", raw.vol)))?
        .min(100);

    let mute = match raw.mute.as_str() {
        "1" => true,
        "0" => false,
        other => {
            return Err(ControlPointError::LinkPlayError(format!(
                "Invalid mute value: {}",
                other
            )));
        }
    };

    let track_index = raw
        .plicurr
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|idx| *idx > 0);

    Ok(LinkPlayStatus {
        state_raw: raw.status,
        curpos_ms,
        totlen_ms,
        track_index,
        volume,
        mute,
    })
}
