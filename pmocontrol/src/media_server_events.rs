use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{IpAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use crossbeam_channel::{Receiver, Sender, unbounded};
use tracing::{debug, error, info, warn};
use ureq::{Agent, http};
use xmltree::{Element, XMLNode};

use crate::events::MediaServerEventBus;
use crate::media_server::MusicServer;
use crate::model::MediaServerEvent;
use crate::registry::DeviceRegistry;
use crate::upnp_clients::resolve_control_url;
use crate::{DeviceId, DeviceIdentity, DeviceOnline};

const SUBSCRIPTION_TIMEOUT_SECS: u64 = 300;
const RENEWAL_SAFETY_MARGIN_SECS: u64 = 60;
const HTTP_READ_TIMEOUT_SECS: u64 = 5;
const WORKER_LOOP_INTERVAL_MILLIS: u64 = 250;
const RETRY_DELAY_SECS: u64 = 15;
const SUBSCRIPTION_RESET_DELAY_SECS: u64 = 5;

/// Launch the media server event runtime responsible for subscribing
/// to ContentDirectory updates and forwarding notifications on the bus.
pub(crate) fn spawn_media_server_event_runtime(
    registry: Arc<RwLock<DeviceRegistry>>,
    bus: MediaServerEventBus,
    timeout_secs: u64,
) -> io::Result<()> {
    let listener = TcpListener::bind("0.0.0.0:0")?;
    let listener_addr = listener
        .local_addr()
        .context("Failed to read listener address")
        .map_err(io_from_anyhow)?;

    info!("MediaServer event listener bound on {}", listener_addr);

    let (notify_tx, notify_rx) = unbounded::<IncomingNotify>();
    thread::Builder::new()
        .name("media-server-event-http".into())
        .spawn(move || run_http_listener(listener, notify_tx))?;

    let worker = MediaServerEventWorker::new(
        registry,
        bus,
        Duration::from_secs(timeout_secs.max(1)),
        notify_rx,
        listener_addr.port(),
    );

    thread::Builder::new()
        .name("media-server-event-worker".into())
        .spawn(move || worker.run())
        .map(|_| ())
}

fn io_from_anyhow(err: anyhow::Error) -> io::Error {
    io::Error::new(io::ErrorKind::Other, err)
}

struct IncomingNotify {
    path: String,
    sid: Option<String>,
    body: Vec<u8>,
}

impl IncomingNotify {
    fn validate_sid(&self, expected: &Option<String>) -> bool {
        match (&self.sid, expected) {
            (Some(received), Some(expected)) => expected.eq_ignore_ascii_case(received),
            _ => false,
        }
    }
}

/// Manages retry timing for subscription operations
struct RetryPolicy {
    retry_after: Instant,
}

impl RetryPolicy {
    fn new() -> Self {
        Self {
            retry_after: Instant::now(),
        }
    }

    fn should_retry(&self) -> bool {
        Instant::now() >= self.retry_after
    }

    fn defer_retry(&mut self) {
        self.retry_after = Instant::now() + Duration::from_secs(RETRY_DELAY_SECS);
    }

    fn schedule_soon(&mut self) {
        self.retry_after = Instant::now() + Duration::from_secs(SUBSCRIPTION_RESET_DELAY_SECS);
    }
}

fn run_http_listener(listener: TcpListener, notify_tx: Sender<IncomingNotify>) {
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                if let Err(err) =
                    stream.set_read_timeout(Some(Duration::from_secs(HTTP_READ_TIMEOUT_SECS)))
                {
                    warn!("Failed to set read timeout on notify connection: {}", err);
                }

                match read_http_request(&mut stream) {
                    Ok(request) => {
                        if request.method != "NOTIFY" {
                            let _ = write_http_response(&mut stream, 405, "Method Not Allowed");
                            continue;
                        }

                        let notify = IncomingNotify {
                            path: request.path,
                            sid: request.headers.get("sid").cloned(),
                            body: request.body,
                        };

                        if notify_tx.send(notify).is_err() {
                            warn!("Dropping notify event because worker channel is closed");
                        }
                        let _ = write_http_response(&mut stream, 200, "OK");
                    }
                    Err(err) => {
                        warn!("Failed to parse incoming notify request: {}", err);
                        let _ = write_http_response(&mut stream, 400, "Bad Request");
                    }
                }
            }
            Err(err) => {
                warn!("Incoming notify connection failed: {}", err);
            }
        }
    }
}

struct HttpRequest {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

fn read_http_request(stream: &mut TcpStream) -> io::Result<HttpRequest> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line)? == 0 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "missing request line",
        ));
    }

    let request_line = request_line.trim_end_matches(&['\r', '\n'][..]);
    let mut parts = request_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing method"))?
        .to_ascii_uppercase();
    let path = parts
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing path"))?
        .to_string();

    // Headers
    let mut headers = HashMap::new();
    loop {
        let mut line = String::new();
        let len = reader.read_line(&mut line)?;
        if len == 0 {
            break;
        }
        let trimmed = line.trim_end_matches(&['\r', '\n'][..]);
        if trimmed.is_empty() {
            break;
        }
        if let Some((name, value)) = trimmed.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }

    let content_length: usize = headers
        .get("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body)?;

    Ok(HttpRequest {
        method,
        path,
        headers,
        body,
    })
}

fn write_http_response(stream: &mut TcpStream, status: u16, message: &str) -> io::Result<()> {
    let response = format!(
        "HTTP/1.1 {} {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        status, message
    );
    stream.write_all(response.as_bytes())
}

struct MediaServerEventWorker {
    registry: Arc<RwLock<DeviceRegistry>>,
    bus: MediaServerEventBus,
    http_timeout: Duration,
    notify_rx: Receiver<IncomingNotify>,
    listener_port: u16,
    subscriptions: HashMap<DeviceId, SubscriptionState>,
    path_index: HashMap<String, DeviceId>,
}

impl MediaServerEventWorker {
    fn new(
        registry: Arc<RwLock<DeviceRegistry>>,
        bus: MediaServerEventBus,
        http_timeout: Duration,
        notify_rx: Receiver<IncomingNotify>,
        listener_port: u16,
    ) -> Self {
        Self {
            registry,
            bus,
            http_timeout,
            notify_rx,
            listener_port,
            subscriptions: HashMap::new(),
            path_index: HashMap::new(),
        }
    }

    fn run(mut self) {
        loop {
            self.drain_notifications();
            self.refresh_servers();
            self.renew_expiring();
            thread::sleep(Duration::from_millis(WORKER_LOOP_INTERVAL_MILLIS));
        }
    }

    fn drain_notifications(&mut self) {
        while let Ok(notify) = self.notify_rx.try_recv() {
            self.handle_notification(notify);
        }
    }

    fn refresh_servers(&mut self) {
        let servers = {
            let reg = self.registry.read().unwrap();
            match reg.list_servers() {
                Ok(servers) => servers,
                Err(e) => {
                    error!("Failed to list servers: {}", e);
                    return;
                }
            }
        };

        let mut active: HashSet<DeviceId> = HashSet::new();

        for server in servers {
            if !server.is_online() || !server.has_content_directory() {
                continue;
            }

            active.insert(server.id());
            let entry = self
                .subscriptions
                .entry(server.id())
                .or_insert_with(|| SubscriptionState::from_music_server(&server));
            entry.update_from_server(&server);
            self.path_index
                .insert(entry.callback_path.clone(), entry.device_id.clone());

            if entry.event_sub_url.is_none() {
                if entry.retry_policy.should_retry() {
                    match fetch_event_sub_url(&entry.location, self.http_timeout) {
                        Ok(Some(url)) => {
                            debug!(
                                server = entry.friendly_name.as_str(),
                                callback = url.as_str(),
                                "ContentDirectory eventSub URL resolved"
                            );
                            entry.event_sub_url = Some(url);
                            entry.retry_policy = RetryPolicy::new();
                        }
                        Ok(None) => {
                            debug!(
                                server = entry.friendly_name.as_str(),
                                "No ContentDirectory eventSub URL found"
                            );
                            entry.retry_policy.defer_retry();
                            continue;
                        }
                        Err(err) => {
                            warn!(
                                server = entry.friendly_name.as_str(),
                                error = %err,
                                "Failed to fetch ContentDirectory eventSub URL"
                            );
                            entry.retry_policy.defer_retry();
                            continue;
                        }
                    }
                } else {
                    continue;
                }
            }

            if entry.sid.is_none() && entry.retry_policy.should_retry() {
                if let Err(err) =
                    Self::subscribe_entry(self.listener_port, self.http_timeout, entry)
                {
                    warn!(
                        server = entry.friendly_name.as_str(),
                        error = %err,
                        "ContentDirectory SUBSCRIBE failed"
                    );
                    entry.retry_policy.defer_retry();
                }
            }
        }

        let stale_ids: Vec<DeviceId> = self
            .subscriptions
            .keys()
            .filter(|id| !active.contains(*id))
            .cloned()
            .collect();

        for id in stale_ids {
            if let Some(mut entry) = self.subscriptions.remove(&id) {
                self.path_index.remove(&entry.callback_path);
                Self::unsubscribe_entry(self.http_timeout, &mut entry);
            }
        }
    }

    fn renew_expiring(&mut self) {
        let now = Instant::now();
        let mut to_renew = Vec::new();
        for (id, entry) in self.subscriptions.iter() {
            if let Some(exp) = entry.expires_at {
                if exp <= now + Duration::from_secs(RENEWAL_SAFETY_MARGIN_SECS) {
                    to_renew.push(id.clone());
                }
            }
        }

        for id in to_renew {
            if let Some(entry) = self.subscriptions.get_mut(&id) {
                if let Err(err) = Self::renew_entry(self.http_timeout, entry) {
                    warn!(
                        server = entry.friendly_name.as_str(),
                        error = %err,
                        "Failed to renew ContentDirectory subscription"
                    );
                    entry.reset_subscription();
                }
            }
        }
    }

    fn subscribe_entry(
        listener_port: u16,
        http_timeout: Duration,
        entry: &mut SubscriptionState,
    ) -> Result<()> {
        let event_url = entry
            .event_sub_url
            .as_ref()
            .context("EventSub URL missing for server")?;

        let (host_header, timeout_header) = build_subscribe_headers(event_url)?;

        let (remote_host, remote_port) =
            parse_host_port(event_url).context("Cannot extract host for callback")?;
        let local_ip = determine_local_ip(&remote_host, remote_port)
            .context("Cannot determine local IP for callback")?;

        let callback_url = format!(
            "http://{}:{}{}",
            format_ip(&local_ip),
            listener_port,
            entry.callback_path
        );

        debug!(
            server = entry.friendly_name.as_str(),
            callback = callback_url.as_str(),
            "Subscribing to ContentDirectory events"
        );

        let callback_header = format!("<{}>", callback_url);

        let request = http::Request::builder()
            .method("SUBSCRIBE")
            .uri(event_url)
            .header("HOST", host_header)
            .header("CALLBACK", callback_header)
            .header("NT", "upnp:event")
            .header("TIMEOUT", timeout_header)
            .body(())
            .map_err(anyhow::Error::new)?;

        let response = build_agent(http_timeout).run(request)?;
        if !response.status().is_success() {
            anyhow::bail!("SUBSCRIBE returned HTTP {}", response.status());
        }

        let sid = response
            .headers()
            .get("SID")
            .and_then(|value| value.to_str().ok())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("SUBSCRIBE response missing SID"))?;
        let timeout = parse_timeout(
            response
                .headers()
                .get("TIMEOUT")
                .and_then(|value| value.to_str().ok()),
        )
        .unwrap_or(Duration::from_secs(SUBSCRIPTION_TIMEOUT_SECS));

        entry.sid = Some(sid);
        entry.expires_at = Some(Instant::now() + timeout);
        entry.retry_policy.schedule_soon();

        info!(
            server = entry.friendly_name.as_str(),
            "Subscribed to ContentDirectory events (timeout {}s)",
            timeout.as_secs()
        );

        Ok(())
    }

    fn renew_entry(http_timeout: Duration, entry: &mut SubscriptionState) -> Result<()> {
        let event_url = entry
            .event_sub_url
            .as_ref()
            .context("EventSub URL missing for renew")?;
        let sid = entry
            .sid
            .as_ref()
            .cloned()
            .context("SID missing for renew")?;

        let (host_header, timeout_header) = build_subscribe_headers(event_url)?;

        let request = http::Request::builder()
            .method("SUBSCRIBE")
            .uri(event_url)
            .header("HOST", host_header)
            .header("TIMEOUT", timeout_header)
            .header("SID", sid.clone())
            .body(())
            .map_err(anyhow::Error::new)?;

        let response = build_agent(http_timeout).run(request)?;
        if !response.status().is_success() {
            anyhow::bail!("SUBSCRIBE renewal failed with {}", response.status());
        }

        let timeout = parse_timeout(
            response
                .headers()
                .get("TIMEOUT")
                .and_then(|value| value.to_str().ok()),
        )
        .unwrap_or(Duration::from_secs(SUBSCRIPTION_TIMEOUT_SECS));
        entry.expires_at = Some(Instant::now() + timeout);
        debug!(
            server = entry.friendly_name.as_str(),
            "Renewed ContentDirectory subscription"
        );
        Ok(())
    }

    fn unsubscribe_entry(http_timeout: Duration, entry: &mut SubscriptionState) {
        let Some(event_url) = entry.event_sub_url.as_ref() else {
            return;
        };
        let Some(sid) = entry.sid.take() else {
            return;
        };
        let Some((remote_host, remote_port)) = parse_host_port(event_url) else {
            return;
        };

        let host_header = format!("{}:{}", remote_host, remote_port);
        let request = match http::Request::builder()
            .method("UNSUBSCRIBE")
            .uri(event_url)
            .header("HOST", host_header)
            .header("SID", sid)
            .body(())
            .map_err(anyhow::Error::new)
        {
            Ok(req) => req,
            Err(err) => {
                warn!(
                    server = entry.friendly_name.as_str(),
                    error = %err,
                    "Failed to build UNSUBSCRIBE request"
                );
                return;
            }
        };

        match build_agent(http_timeout).run(request) {
            Ok(response) => {
                if response.status().is_success() {
                    debug!(
                        server = entry.friendly_name.as_str(),
                        "Unsubscribed from ContentDirectory events"
                    );
                } else {
                    warn!(
                        server = entry.friendly_name.as_str(),
                        status = %response.status(),
                        "UNSUBSCRIBE returned non-success status"
                    );
                }
            }
            Err(err) => {
                warn!(
                    server = entry.friendly_name.as_str(),
                    error = %err,
                    "UNSUBSCRIBE request failed"
                );
            }
        }
    }

    fn handle_notification(&mut self, notify: IncomingNotify) {
        let Some(server_id) = self.path_index.get(&notify.path).cloned() else {
            debug!("Dropping notify for unknown path {}", notify.path);
            return;
        };

        let Some(entry) = self.subscriptions.get(&server_id) else {
            return;
        };

        if !notify.validate_sid(&entry.sid) {
            debug!(
                server = entry.friendly_name.as_str(),
                expected_sid = entry.sid.as_deref().unwrap_or("none"),
                received_sid = notify.sid.as_deref().unwrap_or("none"),
                "Ignoring notify with mismatched SID"
            );
            return;
        }

        for event in parse_notify_payload(&entry.device_id, &notify.body) {
            match &event {
                MediaServerEvent::GlobalUpdated {
                    system_update_id, ..
                } => {
                    debug!(
                        server = entry.friendly_name.as_str(),
                        update_id = system_update_id.unwrap_or_default(),
                        "Broadcasting MediaServerEvent::GlobalUpdated"
                    );
                }
                MediaServerEvent::ContainersUpdated { container_ids, .. } => {
                    debug!(
                        server = entry.friendly_name.as_str(),
                        changed_containers = container_ids.join(",").as_str(),
                        "Broadcasting MediaServerEvent::ContainersUpdated"
                    );
                }
                MediaServerEvent::Online { .. } | MediaServerEvent::Offline { .. } => {
                    // Online/Offline events are generated from SSDP discovery, not from notify payloads
                }
            }
            self.bus.broadcast(event);
        }
    }
}

struct SubscriptionState {
    device_id: DeviceId,
    location: String,
    friendly_name: String,
    event_sub_url: Option<String>,
    sid: Option<String>,
    expires_at: Option<Instant>,
    callback_path: String,
    retry_policy: RetryPolicy,
}

impl SubscriptionState {
    /// Creates a new subscription state from a MusicServer
    fn from_music_server(server: &MusicServer) -> Self {
        Self {
            callback_path: build_callback_path(&server.id()),
            device_id: server.id(),
            location: server.location().to_string(),
            friendly_name: server.friendly_name().to_string(),
            event_sub_url: None,
            sid: None,
            expires_at: None,
            retry_policy: RetryPolicy::new(),
        }
    }

    /// Updates the subscription state from a MusicServer
    fn update_from_server(&mut self, server: &MusicServer) {
        let new_location = server.location();
        if self.location != new_location {
            // Location changed - invalidate subscription
            self.event_sub_url = None;
            self.sid = None;
            self.expires_at = None;
            self.retry_policy = RetryPolicy::new();
        }
        self.device_id = server.id();
        self.location = new_location.to_string();
        self.friendly_name = server.friendly_name().to_string();
    }

    fn reset_subscription(&mut self) {
        self.sid = None;
        self.expires_at = None;
        self.retry_policy.schedule_soon();
    }
}

/// Builds HTTP headers common to SUBSCRIBE requests
fn build_subscribe_headers(event_url: &str) -> Result<(String, String)> {
    let (remote_host, remote_port) =
        parse_host_port(event_url).context("Cannot extract host for SUBSCRIBE")?;
    let host_header = format!("{}:{}", remote_host, remote_port);
    let timeout_header = format!("Second-{}", SUBSCRIPTION_TIMEOUT_SECS);
    Ok((host_header, timeout_header))
}

fn build_callback_path(id: &DeviceId) -> String {
    let mut sanitized = String::new();
    for ch in id.0.chars() {
        if ch.is_ascii_alphanumeric() {
            sanitized.push(ch);
        } else {
            sanitized.push('_');
        }
    }

    let mut hasher = DefaultHasher::new();
    id.hash(&mut hasher);
    let suffix = hasher.finish();

    format!("/media-server-events/{}-{:x}", sanitized, suffix)
}

fn parse_notify_payload(server_id: &DeviceId, body: &[u8]) -> Vec<MediaServerEvent> {
    let mut events = Vec::new();
    let reader = std::io::Cursor::new(body);
    let Ok(root) = Element::parse(reader) else {
        warn!(
            server = server_id.0.as_str(),
            "Failed to parse ContentDirectory notify payload"
        );
        return events;
    };

    let mut system_update_id: Option<u32> = None;
    let mut container_ids: Vec<String> = Vec::new();

    // Navigate through property elements
    for property in xml_children(&root) {
        for child in xml_children(property) {
            match child.name.as_str() {
                "SystemUpdateID" => {
                    system_update_id = child
                        .get_text()
                        .and_then(|text| text.trim().parse::<u32>().ok());
                }
                "ContainerUpdateIDs" => {
                    if let Some(text) = child.get_text() {
                        container_ids = parse_container_update_ids(text.as_ref());
                    }
                }
                _ => {}
            }
        }
    }

    if system_update_id.is_some() {
        events.push(MediaServerEvent::GlobalUpdated {
            server_id: server_id.clone(),
            system_update_id,
        });
    }

    if !container_ids.is_empty() {
        events.push(MediaServerEvent::ContainersUpdated {
            server_id: server_id.clone(),
            container_ids,
        });
    }

    events
}

/// Helper to iterate over XML element children (filters out non-element nodes)
fn xml_children(element: &Element) -> impl Iterator<Item = &Element> {
    element.children.iter().filter_map(|node| match node {
        XMLNode::Element(elem) => Some(elem),
        _ => None,
    })
}

fn parse_container_update_ids(raw: &str) -> Vec<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    if trimmed.contains('$') {
        trimmed
            .split(',')
            .filter_map(|part| part.split('$').next())
            .map(|id| id.trim().to_string())
            .filter(|id| !id.is_empty())
            .collect()
    } else {
        let mut ids = Vec::new();
        let mut tokens = trimmed
            .split(',')
            .map(|t| t.trim())
            .filter(|t| !t.is_empty());
        loop {
            let Some(id) = tokens.next() else {
                break;
            };
            ids.push(id.to_string());
            tokens.next(); // Skip the accompanying UpdateID
        }
        ids
    }
}

fn child_text(element: &Element, name: &str) -> Option<String> {
    xml_children(element)
        .find(|child| child.name == name)
        .and_then(|child| child.get_text().map(|cow| cow.into_owned()))
}

fn fetch_event_sub_url(location: &str, timeout: Duration) -> Result<Option<String>> {
    let agent = Agent::config_builder()
        .timeout_global(Some(timeout))
        .build();
    let agent: Agent = agent.into();
    let response = agent
        .get(location)
        .call()
        .with_context(|| format!("HTTP error when fetching description at {}", location))?;
    let (_parts, body) = response.into_parts();
    let mut reader = BufReader::new(body.into_reader());
    let root = Element::parse(&mut reader)?;

    let device = match root.get_child("device") {
        Some(device) => device,
        None => return Ok(None),
    };
    let service_list = match device.get_child("serviceList") {
        Some(list) => list,
        None => return Ok(None),
    };

    for service in xml_children(service_list) {
        let Some(service_type) = child_text(service, "serviceType") else {
            continue;
        };
        if !service_type
            .to_ascii_lowercase()
            .contains("urn:schemas-upnp-org:service:contentdirectory:")
        {
            continue;
        }
        if let Some(event_sub) = child_text(service, "eventSubURL") {
            return Ok(Some(resolve_control_url(location, &event_sub)));
        }
    }

    Ok(None)
}

fn parse_timeout(raw: Option<&str>) -> Option<Duration> {
    let value = raw?;
    let lower = value.trim().to_ascii_lowercase();
    if lower == "second-infinite" {
        return Some(Duration::from_secs(SUBSCRIPTION_TIMEOUT_SECS));
    }
    if let Some(idx) = lower.find("second-") {
        let number = &lower[idx + 7..];
        if let Ok(seconds) = number.parse::<u64>() {
            return Some(Duration::from_secs(seconds));
        }
    }
    None
}

fn parse_host_port(url: &str) -> Option<(String, u16)> {
    let default_port = if url.to_ascii_lowercase().starts_with("https://") {
        443
    } else {
        80
    };
    let (_, rest) = url.split_once("://")?;
    let mut parts = rest.splitn(2, '/');
    let authority = parts.next()?.trim();
    if authority.starts_with('[') {
        let end = authority.find(']')?;
        let host = &authority[1..end];
        let remainder = authority.get(end + 1..).unwrap_or("");
        let port = if let Some(stripped) = remainder.strip_prefix(':') {
            stripped.parse().unwrap_or(default_port)
        } else {
            default_port
        };
        Some((host.to_string(), port))
    } else if let Some((host, port)) = authority.split_once(':') {
        Some((host.to_string(), port.parse().ok()?))
    } else {
        Some((authority.to_string(), default_port))
    }
}

fn determine_local_ip(remote_host: &str, remote_port: u16) -> io::Result<IpAddr> {
    let is_ipv6 = remote_host.contains(':') && !remote_host.contains('.');
    let target = if is_ipv6 {
        format!(
            "[{}]:{}",
            remote_host.trim_matches(|c| c == '[' || c == ']'),
            remote_port
        )
    } else {
        format!("{}:{}", remote_host, remote_port)
    };
    let bind_addr = if is_ipv6 { "[::]:0" } else { "0.0.0.0:0" };
    let socket = UdpSocket::bind(bind_addr)?;
    socket.connect(&target)?;
    Ok(socket.local_addr()?.ip())
}

fn format_ip(ip: &IpAddr) -> String {
    match ip {
        IpAddr::V4(v4) => v4.to_string(),
        IpAddr::V6(v6) => format!("[{}]", v6),
    }
}

fn build_agent(timeout: Duration) -> Agent {
    Agent::config_builder()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .allow_non_standard_methods(true)
        .build()
        .into()
}
