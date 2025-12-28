use std::char;
use std::collections::HashMap;
use std::fmt;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use tracing::debug;
use ureq::Agent;

use crate::DeviceIdentity;
use crate::capabilities::{
    PlaybackPosition, PlaybackPositionInfo, PlaybackState, PlaybackStatus, TransportControl,
    VolumeControl,
};
use crate::errors::ControlPointError;
use crate::model::{RendererInfo};

const DEFAULT_HTTP_TIMEOUT_SECS: u64 = 3;
const STATUS_COMMAND: &str = "getPlayerStatus";

/// Renderer backend for devices exposing the LinkPlay HTTP API.
#[derive(Clone)]
pub struct LinkPlayRenderer {
    host: String,
    timeout: Duration,
}

impl fmt::Debug for LinkPlayRenderer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LinkPlayRenderer")
            .field("host", &self.host)
            .finish()
    }
}

impl LinkPlayRenderer {
    /// Build a LinkPlay backend from a registry snapshot.
    pub fn from_renderer_info(info: RendererInfo) -> Result<Self, ControlPointError> {
        let host = extract_linkplay_host(&info.location())
            .ok_or_else(|| ControlPointError::LinkPlayError(format!("Renderer {} has no valid LOCATION host", info.udn())))?;

        Ok(Self {
            host,
            timeout: Duration::from_secs(DEFAULT_HTTP_TIMEOUT_SECS),
        })
    }

    fn agent(&self) -> Agent {
        build_agent(self.timeout)
    }

    fn send_player_command(&self, command: &str) -> Result<(), ControlPointError> {
        let url = format!(
            "http://{}/httpapi.asp?command=setPlayerCmd:{}",
            self.host, command
        );
        self.agent()
            .get(&url)
            .call()
            .map_err(|_| ControlPointError::ArilycTcpError(format!("LinkPlay command {} failed for {}", command, self.host)))?;
        Ok(())
    }

    fn fetch_status(&self) -> Result<LinkPlayStatus, ControlPointError> {
        fetch_status_for_host(&self.host, self.timeout)
    }
}

impl TransportControl for LinkPlayRenderer {
    fn play_uri(&self, uri: &str, _meta: &str) -> Result<(), ControlPointError> {
        let encoded = percent_encode(uri);
        self.send_player_command(&format!("play:{}", encoded))
    }

    fn play(&self) -> Result<(), ControlPointError> {
        self.send_player_command("resume")
    }

    fn pause(&self) -> Result<(), ControlPointError> {
        self.send_player_command("pause")
    }

    fn stop(&self) -> Result<(), ControlPointError> {
        self.send_player_command("stop")
    }

    fn seek_rel_time(&self, hhmmss: &str) -> Result<(), ControlPointError> {
        let secs = parse_hhmmss_to_secs(hhmmss)
            .ok_or_else(|| ControlPointError::LinkPlayError(format!("Invalid seek position format: {}", hhmmss)))?;
        self.send_player_command(&format!("seek:{}", secs))
    }
}

impl VolumeControl for LinkPlayRenderer {
    fn volume(&self) -> Result<u16, ControlPointError> {
        Ok(self.fetch_status()?.volume)
    }

    fn set_volume(&self, v: u16) -> Result<(), ControlPointError> {
        let value = v.min(100);
        self.send_player_command(&format!("vol:{}", value))
    }

    fn mute(&self) -> Result<bool, ControlPointError> {
        Ok(self.fetch_status()?.mute)
    }

    fn set_mute(&self, m: bool) -> Result<(), ControlPointError> {
        self.send_player_command(if m { "mute:1" } else { "mute:0" })
    }
}

impl PlaybackStatus for LinkPlayRenderer {
    fn playback_state(&self) -> Result<PlaybackState, ControlPointError> {
        Ok(self.fetch_status()?.playback_state())
    }
}

impl PlaybackPosition for LinkPlayRenderer {
    fn playback_position(&self) -> Result<PlaybackPositionInfo, ControlPointError> {
        Ok(self.fetch_status()?.position_info())
    }
}

/// Detect whether a renderer exposes the LinkPlay HTTP API.
pub fn detect_linkplay_http(location: &str, timeout: Duration) -> bool {
    let Some(host) = extract_linkplay_host(location) else {
        return false;
    };

    match fetch_status_for_host(&host, timeout) {
        Ok(_) => true,
        Err(err) => {
            debug!(
                "LinkPlay detection failed for {} (host={}): {}",
                location, host, err
            );
            false
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

fn fetch_status_for_host(host: &str, timeout: Duration) -> Result<LinkPlayStatus, ControlPointError> {
    let url = format!("http://{}/httpapi.asp?command={}", host, STATUS_COMMAND);
    let mut response = build_agent(timeout)
        .get(&url)
        .call()
        .map_err(|_| ControlPointError::ArilycTcpError(format!("HTTP request failed for LinkPlay status on {}", host)))?;

    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|e| ControlPointError::ArilycTcpError(format!("Failed to read LinkPlay status body : {}",e)))?;

    parse_linkplay_status(&body)
}

fn build_agent(timeout: Duration) -> Agent {
    Agent::config_builder()
        .timeout_global(Some(timeout))
        .build()
        .into()
}

#[derive(Clone, Debug)]
struct LinkPlayStatus {
    state_raw: String,
    curpos_ms: u64,
    totlen_ms: u64,
    track_index: Option<u32>,
    volume: u16,
    mute: bool,
}

impl LinkPlayStatus {
    fn playback_state(&self) -> PlaybackState {
        match self.state_raw.as_str() {
            "play" => PlaybackState::Playing,
            "pause" => PlaybackState::Paused,
            "stop" => PlaybackState::Stopped,
            "load" => PlaybackState::Transitioning,
            other => PlaybackState::Unknown(other.to_string()),
        }
    }

    fn position_info(&self) -> PlaybackPositionInfo {
        PlaybackPositionInfo {
            track: self.track_index,
            rel_time: Some(format_hms(self.curpos_ms / 1000)),
            abs_time: None,
            track_duration: if self.totlen_ms > 0 {
                Some(format_hms(self.totlen_ms / 1000))
            } else {
                None
            },
            track_metadata: None,
            track_uri: None,
        }
    }
}

fn parse_linkplay_status(body: &str) -> Result<LinkPlayStatus, ControlPointError> {
    let mut map = parse_flat_json(body)?;

    let state_raw = map
        .remove("status")
        .ok_or_else(|| ControlPointError::ArilycTcpError(format!("LinkPlay status missing `status` field")))?;

    let curpos_ms = parse_u64_field(&map, "curpos")?;
    let totlen_ms = parse_u64_field(&map, "totlen")?;
    let volume = parse_u16_field(&map, "vol")?;
    let mute = match map.get("mute").map(|s| s.as_str()) {
        Some("1") => true,
        Some("0") => false,
        Some(other) => {
            return Err(ControlPointError::ArilycTcpError(format!("Invalid LinkPlay mute value: {}", other)));
        }
        None => return Err(ControlPointError::arilyc_tcp_error("LinkPlay status missing `mute` field")),
    };

    let track_index = map
        .get("plicurr")
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|idx| *idx > 0);

    Ok(LinkPlayStatus {
        state_raw,
        curpos_ms,
        totlen_ms,
        track_index,
        volume,
        mute,
    })
}

fn parse_u64_field(map: &HashMap<String, String>, key: &str) -> Result<u64, ControlPointError> {
    let raw = map
        .get(key)
        .ok_or_else(|| ControlPointError::ArilycTcpError(format!("LinkPlay status missing `{}` field", key)))?;
    raw.parse::<u64>()
        .map_err(|_| ControlPointError::LinkPlayError(format!("Invalid `{}` value: {}", key, raw)))
}

fn parse_u16_field(map: &HashMap<String, String>, key: &str) -> Result<u16, ControlPointError> {
    let raw = map
        .get(key)
        .ok_or_else(|| ControlPointError::ArilycTcpError(format!("LinkPlay status missing `{}` field", key)))?;
    let value = raw
        .parse::<u16>()
        .map_err(|_| ControlPointError::LinkPlayError(format!("Invalid `{}` value: {}", key, raw)))?;
    Ok(value.min(100))
}

pub(crate) fn parse_flat_json(input: &str) -> Result<HashMap<String, String>, ControlPointError> {
    let mut chars = input.chars().peekable();
    skip_ws(&mut chars);
    if chars.next() != Some('{') {
        return Err(ControlPointError::ArilycTcpError(format!("LinkPlay status is not a JSON object")));
    }

    let mut map = HashMap::new();
    loop {
        skip_ws(&mut chars);
        match chars.peek() {
            Some('}') => {
                chars.next();
                break;
            }
            Some(_) => {}
            None => return Err(ControlPointError::ArilycTcpError(format!("Unexpected end of JSON object"))),
        }

        let key = parse_json_string(&mut chars)?;
        skip_ws(&mut chars);
        expect_char(&mut chars, ':')?;
        skip_ws(&mut chars);
        let value = parse_json_value(&mut chars)?;
        map.insert(key, value);
        skip_ws(&mut chars);

        match chars.peek() {
            Some(',') => {
                chars.next();
                continue;
            }
            Some('}') => {
                chars.next();
                break;
            }
            Some(other) => {
                return Err(ControlPointError::ArilycTcpError(format!(
                    "Unexpected character '{}' while parsing JSON",
                    other
                )));
            }
            None => return Err(ControlPointError::ArilycTcpError(format!("Unexpected end of JSON while parsing fields"))),
        }
    }

    Ok(map)
}

fn parse_json_value(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Result<String, ControlPointError> {
    match chars.peek() {
        Some('"') => parse_json_string(chars),
        Some(ch) if ch.is_ascii_digit() || *ch == '-' => parse_json_number(chars),
        Some('t') => {
            expect_literal(chars, "true")?;
            Ok("true".to_string())
        }
        Some('f') => {
            expect_literal(chars, "false")?;
            Ok("false".to_string())
        }
        _ => Err(ControlPointError::ArilycTcpError(format!("Unsupported JSON value in LinkPlay status"))),
    }
}

fn parse_json_string(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Result<String, ControlPointError> {
    if chars.next() != Some('"') {
        return Err(ControlPointError::ArilycTcpError(format!("Expected string")));
    }

    let mut out = String::new();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => return Ok(out),
            '\\' => {
                let escaped = chars.next().ok_or_else(|| ControlPointError::ArilycTcpError(format!("Invalid escape")))?;
                match escaped {
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    '/' => out.push('/'),
                    'b' => out.push('\u{0008}'),
                    'f' => out.push('\u{000C}'),
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    'u' => {
                        let mut hex = String::with_capacity(4);
                        for _ in 0..4 {
                            let h = chars.next().ok_or_else(|| ControlPointError::ArilycTcpError(format!("Invalid \\u escape")))?;
                            hex.push(h);
                        }
                        let code = u16::from_str_radix(&hex, 16)
                            .map_err(|e| ControlPointError::ArilycTcpError(format!("Invalid unicode escape: {}", hex)))?;
                        if let Some(c) = char::from_u32(code as u32) {
                            out.push(c);
                        } else {
                            return Err(ControlPointError::ArilycTcpError(format!("Invalid unicode code point: {}", code)));
                        }
                    }
                    other => return Err(ControlPointError::ArilycTcpError(format!("Unsupported escape: {}", other))),
                }
            }
            other => out.push(other),
        }
    }

    Err(ControlPointError::ArilycTcpError(format!("Unterminated JSON string")))
}

fn parse_json_number(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Result<String, ControlPointError> {
    let mut out = String::new();

    if matches!(chars.peek(), Some('-')) {
        out.push('-');
        chars.next();
    }

    while let Some(ch) = chars.peek() {
        if ch.is_ascii_digit() || *ch == '.' {
            out.push(*ch);
            chars.next();
        } else {
            break;
        }
    }

    if out.is_empty() || out == "-" {
        return Err(ControlPointError::ArilycTcpError(format!("Invalid number")));
    }

    Ok(out)
}

fn expect_literal(
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    literal: &str,
) -> Result<(), ControlPointError> {
    for expected in literal.chars() {
        match chars.next() {
            Some(ch) if ch == expected => {}
            _ => return Err(ControlPointError::ArilycTcpError(format!("Invalid literal while parsing JSON"))),
        }
    }
    Ok(())
}

fn expect_char(chars: &mut std::iter::Peekable<std::str::Chars<'_>>, expected: char) -> Result<(), ControlPointError> {
    match chars.next() {
        Some(ch) if ch == expected => Ok(()),
        _ => Err(ControlPointError::ArilycTcpError(format!("Missing '{}' while parsing JSON", expected))),
    }
}

fn skip_ws(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    while matches!(chars.peek(), Some(ch) if ch.is_whitespace()) {
        chars.next();
    }
}

fn percent_encode(input: &str) -> String {
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

fn parse_hhmmss_to_secs(s: &str) -> Option<u64> {
    let parts: Vec<_> = s.split(':').collect();
    if parts.len() != 3 {
        return None;
    }
    let h: u64 = parts[0].parse().ok()?;
    let m: u64 = parts[1].parse().ok()?;
    let sec: u64 = parts[2].parse().ok()?;
    Some(h * 3600 + m * 60 + sec)
}

fn format_hms(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{:02}:{:02}:{:02}", h, m, s)
}
