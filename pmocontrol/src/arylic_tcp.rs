use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use anyhow::anyhow;
use anyhow::{Context, Result};
use tracing::{debug, warn};

use crate::capabilities::{
    PlaybackPosition, PlaybackPositionInfo, PlaybackState, PlaybackStatus, TransportControl,
    VolumeControl,
};
use crate::linkplay::{extract_linkplay_host, parse_flat_json};
use crate::model::{RendererId, RendererInfo};
use std::time::Instant;

// Garde global pour respecter le délai de 200ms entre commandes
static LAST_COMMAND_TIME: OnceLock<Mutex<Instant>> = OnceLock::new();

fn last_command_time() -> &'static Mutex<Instant> {
    LAST_COMMAND_TIME.get_or_init(|| Mutex::new(Instant::now()))
}


const ARYLIC_TCP_PORT: u16 = 8899;
const PACKET_HEADER: [u8; 4] = [0x18, 0x96, 0x18, 0x20];
const RESERVED_BYTES: [u8; 8] = [0; 8];
const MAX_RESPONSE_ATTEMPTS: usize = 8;
const DEFAULT_TIMEOUT_SECS: u64 = 3;

static DETECTION_CACHE: OnceLock<Mutex<HashMap<String, bool>>> = OnceLock::new();

/// Mode d’attente de réponse pour une commande TCP Arylic.
enum ResponseMode<'a> {
    /// On n’attend aucune réponse (fire-and-forget).
    None,
    /// On attend une réponse, mais si la lecture échoue immédiatement, on traite comme succès.
    Optional(&'a [&'a str]),
    /// On attend une réponse, et l’absence de réponse est une erreur.
    Required(&'a [&'a str]),
}

fn detection_cache() -> &'static Mutex<HashMap<String, bool>> {
    DETECTION_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Probe whether the renderer at the given location exposes the Arylic TCP API.
pub(crate) fn detect_arylic_tcp(location: &str, timeout: Duration) -> bool {
    let Some(host) = extract_linkplay_host(location) else {
        return false;
    };

    if let Ok(cache) = detection_cache().lock() {
        if let Some(result) = cache.get(&host) {
            return *result;
        }
    }

    let detected = match try_detect_tcp(&host, timeout) {
        Ok(_) => true,
        Err(err) => {
            debug!(
                "Arylic TCP detection failed for {} (host={}): {}",
                location, host, err
            );
            false
        }
    };

    if let Ok(mut cache) = detection_cache().lock() {
        cache.insert(host, detected);
    }

    detected
}

fn try_detect_tcp(host: &str, timeout: Duration) -> Result<()> {
    let payload = send_command_required(
        host,
        ARYLIC_TCP_PORT,
        timeout,
        "MCU+INF+GET",
        &["AXX+INF+", "AXX+DEV+"],
    )?;

    if payload.starts_with("AXX+INF+") || payload.starts_with("AXX+DEV+") {
        Ok(())
    } else {
        Err(anyhow!(
            "Unexpected INF response from {}: {}",
            host,
            payload
        ))
    }
}

/// Backend speaking the Arylic TCP control protocol (port 8899).
#[derive(Clone, Debug)]
pub struct ArylicTcpRenderer {
    pub info: RendererInfo,
    host: String,
    port: u16,
    timeout: Duration,
}

impl ArylicTcpRenderer {
    pub fn from_renderer_info(info: RendererInfo) -> Result<Self> {
        let host = extract_linkplay_host(&info.location)
            .ok_or_else(|| anyhow!("Renderer {} has no valid LOCATION host", info.udn))?;

        Ok(Self {
            info,
            host,
            port: ARYLIC_TCP_PORT,
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
        })
    }

    pub fn id(&self) -> &RendererId {
        &self.info.id
    }

    pub fn friendly_name(&self) -> &str {
        &self.info.friendly_name
    }

    fn send_required(&self, cmd: &str, expected: &[&str]) -> Result<String> {
        send_command_required(&self.host, self.port, self.timeout, cmd, expected)
    }

    fn send_optional(&self, cmd: &str, expected: &[&str]) -> Result<Option<String>> {
        send_command_optional(&self.host, self.port, self.timeout, cmd, expected)
    }

    fn send_no_response(&self, cmd: &str) -> Result<()> {
        send_command_no_response(&self.host, self.port, self.timeout, cmd)
    }

    fn fetch_playback_info(&self) -> Result<ArylicPlaybackInfo> {
        let payload = self.send_required("MCU+PINFGET", &["AXX+PLY+INF"])?;
        match parse_playback_info(&payload) {
            Ok(info) => Ok(info),
            Err(err) => {
                debug!(
                    "Failed to parse Arylic playback info for {}: {}",
                    self.host, err
                );
                Err(err)
            }
        }
    }

    fn format_volume_command(value: u16) -> String {
        format!("MCU+VOL+{:03}", value.min(100))
    }

    fn parse_volume_payload(payload: &str) -> Result<u16> {
        let data = payload
            .strip_prefix("AXX+VOL+")
            .ok_or_else(|| anyhow!("Unexpected volume response: {}", payload))?;
        let value: u16 = data
            .trim()
            .parse()
            .with_context(|| format!("Invalid volume value: {}", data))?;
        Ok(value.min(100))
    }

    fn parse_mute_payload(payload: &str) -> Result<bool> {
        let data = payload
            .strip_prefix("AXX+MUT+")
            .ok_or_else(|| anyhow!("Unexpected mute response: {}", payload))?;
        match data.trim() {
            "000" | "0" => Ok(false),
            "001" | "1" => Ok(true),
            other => Err(anyhow!("Invalid mute value: {}", other)),
        }
    }
}

impl TransportControl for ArylicTcpRenderer {
    fn play_uri(&self, _uri: &str, _meta: &str) -> Result<()> {
        Err(anyhow!(
            "Arylic TCP backend does not support direct URL loading. Use UPnP AVTransport SetAVTransportURI instead."
        ))
    }

    fn play(&self) -> Result<()> {
        self.send_no_response("MCU+PLY-PLA")
    }

    fn pause(&self) -> Result<()> {
        let _ = self.send_optional("MCU+PLY-PUS", &["AXX+PLY+"])?;
        Ok(())
    }

    fn stop(&self) -> Result<()> {
        self.send_no_response("MCU+PLY-STP")
    }

    fn seek_rel_time(&self, hhmmss: &str) -> Result<()> {
        let _ = parse_hhmmss(hhmmss)?;
        Err(anyhow!(
            "Arylic TCP seek_rel_time is not implemented yet for this device."
        ))
    }
}

impl VolumeControl for ArylicTcpRenderer {
    fn volume(&self) -> Result<u16> {
        if let Ok(info) = self.fetch_playback_info() {
            if let Some(vol) = info.volume {
                return Ok(vol);
            }
            debug!(
                "Arylic playback info for {} missing volume, falling back to VOL GET",
                self.host
            );
        }
        let payload = self.send_required("MCU+VOL+GET", &["AXX+VOL+"])?;
        Self::parse_volume_payload(&payload)
    }

    fn set_volume(&self, v: u16) -> Result<()> {
        let command = Self::format_volume_command(v);
        let _ = self.send_optional(&command, &["AXX+VOL+"])?;
        Ok(())
    }

    fn mute(&self) -> Result<bool> {
        if let Ok(info) = self.fetch_playback_info() {
            if let Some(mute) = info.mute {
                return Ok(mute);
            }
            debug!(
                "Arylic playback info for {} missing mute, falling back to MUT GET",
                self.host
            );
        }
        let payload = self.send_required("MCU+MUT+GET", &["AXX+MUT+"])?;
        Self::parse_mute_payload(&payload)
    }

    fn set_mute(&self, m: bool) -> Result<()> {
        let command = if m { "MCU+MUT+001" } else { "MCU+MUT+000" };
        let payload = self.send_required(command, &["AXX+MUT+"])?;
        let _ = Self::parse_mute_payload(&payload)?;
        Ok(())
    }
}

impl PlaybackStatus for ArylicTcpRenderer {
    fn playback_state(&self) -> Result<PlaybackState> {
        let info = self.fetch_playback_info()?;
        Ok(info.playback_state())
    }
}

impl PlaybackPosition for ArylicTcpRenderer {
    fn playback_position(&self) -> Result<PlaybackPositionInfo> {
        let info = self.fetch_playback_info()?;
        Ok(info.position_info())
    }
}

#[derive(Debug)]
struct ArylicPlaybackInfo {
    status_raw: String,
    curpos_ms: u64,
    totlen_ms: u64,
    volume: Option<u16>,
    mute: Option<bool>,
    playlist_size: Option<u32>,
    track_index: Option<u32>,
}

impl ArylicPlaybackInfo {
    fn playback_state(&self) -> PlaybackState {
        match self.status_raw.as_str() {
            "play" => PlaybackState::Playing,
            "pause" => PlaybackState::Paused,
            "stop" => PlaybackState::Stopped,
            other => PlaybackState::Unknown(other.to_string()),
        }
    }

    fn position_info(&self) -> PlaybackPositionInfo {
        let track = match (self.track_index, self.playlist_size) {
            (Some(idx), Some(count)) if count > 0 => Some(idx.min(count)),
            (Some(idx), _) => Some(idx),
            _ => None,
        };

        PlaybackPositionInfo {
            track,
            rel_time: Some(format_hms(self.curpos_ms / 1000)),
            abs_time: None,
            track_duration: if self.totlen_ms > 0 {
                Some(format_hms(self.totlen_ms / 1000))
            } else {
                None
            },
        }
    }
}

fn parse_playback_info(payload: &str) -> Result<ArylicPlaybackInfo> {
    let json_blob = payload
        .strip_prefix("AXX+PLY+INF")
        .ok_or_else(|| anyhow!("Unexpected playback info prefix: {}", payload))?;

    let json_blob = json_blob.trim_end_matches('&').trim();
    let map = parse_flat_json(json_blob)?;

    let status_raw = map
        .get("status")
        .cloned()
        .ok_or_else(|| anyhow!("Playback info missing `status` field"))?;

    let curpos_ms = parse_u64_field(&map, "curpos")?;
    let totlen_ms = parse_u64_field(&map, "totlen")?;

    let volume = match map.get("vol") {
        Some(raw) => match raw.parse::<u16>() {
            Ok(value) => Some(value.min(100)),
            Err(err) => {
                debug!("Invalid Arylic `vol` value {}: {}", raw, err);
                None
            }
        },
        None => None,
    };

    let mute = match map.get("mute") {
        Some(value) if value == "1" => Some(true),
        Some(value) if value == "0" => Some(false),
        Some(other) => {
            debug!("Invalid Arylic `mute` value {}", other);
            None
        }
        None => None,
    };

    let playlist_size = map
        .get("plicount")
        .and_then(|raw| match raw.parse::<u32>() {
            Ok(count) if count > 0 => Some(count),
            Ok(_) => None,
            Err(err) => {
                debug!("Invalid Arylic `plicount` value {}: {}", raw, err);
                None
            }
        });

    let track_index = map.get("plicurr").and_then(|raw| match raw.parse::<u32>() {
        Ok(idx) if idx > 0 => Some(idx),
        Ok(_) => None,
        Err(err) => {
            debug!("Invalid Arylic `plicurr` value {}: {}", raw, err);
            None
        }
    });

    Ok(ArylicPlaybackInfo {
        status_raw,
        curpos_ms,
        totlen_ms,
        volume,
        mute,
        playlist_size,
        track_index,
    })
}

fn parse_u64_field(map: &HashMap<String, String>, key: &str) -> Result<u64> {
    let raw = map
        .get(key)
        .ok_or_else(|| anyhow!("Playback info missing `{}` field", key))?;
    raw.parse::<u64>()
        .with_context(|| format!("Invalid `{}` value: {}", key, raw))
}

fn connect(host: &str, port: u16, timeout: Duration) -> Result<TcpStream> {
    if let Ok(mut last_time) = last_command_time().lock() {
        let elapsed = last_time.elapsed();
        if elapsed < Duration::from_millis(200) {
            let wait = Duration::from_millis(200) - elapsed;
            debug!("Waiting {:?} before sending command to respect 200ms interval", wait);
            thread::sleep(wait);
        }
        *last_time = Instant::now();
    }

    let address = if host.contains(':') {
        format!("[{}]:{}", host, port)
    } else {
        format!("{host}:{port}")
    };

    let mut last_err = None;
    for addr in address
        .to_socket_addrs()
        .with_context(|| format!("Failed to resolve {}:{}", host, port))?
    {
        match TcpStream::connect_timeout(&addr, timeout) {
            Ok(stream) => {
                stream
                    .set_read_timeout(Some(timeout))
                    .and_then(|_| stream.set_write_timeout(Some(timeout)))
                    .with_context(|| format!("Failed to set socket timeouts for {}", address))?;
                return Ok(stream);
            }
            Err(err) => {
                last_err = Some((addr, err));
            }
        }
    }

    match last_err {
        Some((addr, err)) => Err(anyhow!(
            "Failed to connect to {} via {}: {}",
            host,
            addr,
            err
        )),
        None => Err(anyhow!("No socket addresses resolved for {}", address)),
    }
}

fn encode_packet(payload: &str) -> Vec<u8> {
    let bytes = payload.as_bytes();
    let len = bytes.len() as u32;
    let checksum = bytes.iter().fold(0u32, |acc, b| acc + (*b as u32));

    let mut out = Vec::with_capacity(4 + 4 + 4 + 8 + bytes.len());
    out.extend_from_slice(&PACKET_HEADER);
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&checksum.to_le_bytes());
    out.extend_from_slice(&RESERVED_BYTES);
    out.extend_from_slice(bytes);
    out
}

fn read_packet(stream: &mut TcpStream) -> Result<String> {
    let mut header = [0u8; 4];
    stream.read_exact(&mut header)?;
    if header != PACKET_HEADER {
        return Err(anyhow!("Invalid Arylic packet header: {:x?}", header));
    }

    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf)?;
    let len = u32::from_le_bytes(len_buf) as usize;

    let mut checksum_buf = [0u8; 4];
    stream.read_exact(&mut checksum_buf)?;
    let expected_checksum = u32::from_le_bytes(checksum_buf);

    let mut reserved = [0u8; 8];
    stream.read_exact(&mut reserved)?;

    let mut payload = vec![0u8; len];
    stream.read_exact(&mut payload)?;

    let actual_checksum = payload.iter().fold(0u32, |acc, b| acc + (*b as u32));
    if actual_checksum != expected_checksum {
        warn!(
            "Arylic payload checksum mismatch: expected={} actual={}",
            expected_checksum, actual_checksum
        );
    }

    Ok(String::from_utf8(payload)?)
}

fn format_hms(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{:02}:{:02}:{:02}", h, m, s)
}

fn parse_hhmmss(value: &str) -> Result<u64> {
    let parts: Vec<_> = value.split(':').collect();
    if parts.len() != 3 {
        return Err(anyhow!(
            "Invalid time format `{}`. Expected HH:MM:SS.",
            value
        ));
    }

    let hours: u64 = parts[0]
        .parse()
        .with_context(|| format!("Invalid hour component in {}", value))?;
    let minutes: u64 = parts[1]
        .parse()
        .with_context(|| format!("Invalid minute component in {}", value))?;
    let seconds: u64 = parts[2]
        .parse()
        .with_context(|| format!("Invalid second component in {}", value))?;

    if minutes > 59 || seconds > 59 {
        return Err(anyhow!(
            "Invalid HH:MM:SS value `{}`. Minutes and seconds must be < 60.",
            value
        ));
    }

    Ok(hours * 3600 + minutes * 60 + seconds)
}

fn send_command_with_mode(
    host: &str,
    port: u16,
    timeout: Duration,
    payload: &str,
    mode: ResponseMode<'_>,
) -> Result<Option<String>> {
    let mut stream = connect(host, port, timeout)?;
    let packet = encode_packet(payload);

    stream
        .write_all(&packet)
        .with_context(|| format!("Failed to write Arylic TCP packet for {}: {}", host, payload))?;
    stream.flush().with_context(|| {
        format!(
            "Failed to flush Arylic TCP stream for {} (command {})",
            host, payload
        )
    })?;

    match mode {
        ResponseMode::None => {
            debug!(
                "Arylic TCP fire-and-forget command sent to {}: {}",
                host, payload
            );
            let _ = stream.shutdown(Shutdown::Write);
            Ok(None)
        }
        ResponseMode::Required(expected) => read_expected_response(&mut stream, host, payload, expected)
            .map(Some),
        ResponseMode::Optional(expected) => {
            for _ in 0..MAX_RESPONSE_ATTEMPTS {
                match read_packet(&mut stream) {
                    Ok(response) => {
                        if expected.iter().any(|p| response.starts_with(p)) {
                            return Ok(Some(response));
                        }
                        debug!(
                            "Ignoring unsolicited Arylic payload from {}: {}",
                            host, response
                        );
                    }
                    Err(err) => {
                        debug!(
                            "No full response for Arylic TCP command {} on {}: {}. Treating as success and relying on PINFGET.",
                            payload, host, err
                        );
                        return Ok(None);
                    }
                }
            }

            Err(anyhow!(
                "No expected response for optional command {} on {}",
                payload,
                host
            ))
        }
    }
}

fn read_expected_response(
    stream: &mut TcpStream,
    host: &str,
    payload: &str,
    expected: &[&str],
) -> Result<String> {
    for _ in 0..MAX_RESPONSE_ATTEMPTS {
        let response = match read_packet(stream) {
            Ok(resp) => resp,
            Err(err) => {
                return Err(anyhow!(
                    "Failed to read Arylic TCP response for {} (command {}): {}",
                    host,
                    payload,
                    err
                ));
            }
        };
        if expected.iter().any(|prefix| response.starts_with(prefix)) {
            return Ok(response);
        }

        debug!(
            "Ignoring unsolicited Arylic payload from {}: {}",
            host, response
        );
    }

    Err(anyhow!(
        "No expected response for command {} on {}",
        payload,
        host
    ))
}

fn send_command_required(
    host: &str,
    port: u16,
    timeout: Duration,
    payload: &str,
    expected: &[&str],
) -> Result<String> {
    match send_command_with_mode(host, port, timeout, payload, ResponseMode::Required(expected))? {
        Some(s) => Ok(s),
        None => Err(anyhow!(
            "Arylic TCP: no response payload for required command {}",
            payload
        )),
    }
}

fn send_command_optional(
    host: &str,
    port: u16,
    timeout: Duration,
    payload: &str,
    expected: &[&str],
) -> Result<Option<String>> {
    send_command_with_mode(host, port, timeout, payload, ResponseMode::Optional(expected))
}

fn send_command_no_response(
    host: &str,
    port: u16,
    timeout: Duration,
    payload: &str,
) -> Result<()> {
    send_command_with_mode(host, port, timeout, payload, ResponseMode::None).map(|_| ())
}
