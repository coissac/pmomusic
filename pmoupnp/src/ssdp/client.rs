/*! 
The PMOMusic SSDP client is a *control point*.
It must **not** bind to UDP port 1900.

Reason:

* The SSDP *server* (UPnP device mode) must listen on 0.0.0.0:1900 for M-SEARCH discovery.
* The SSDP *client* only needs to send M-SEARCH and receive unicast HTTP/200 replies.
* If both client and server bind on 1900 (even with SO_REUSEPORT) the kernel load-balances
  incoming datagrams between sockets. As a result, NOTIFY and HTTP/200 messages are lost
  randomly by the client.

Therefore:

* SSDP server → bind(0.0.0.0:1900), join multicast, answer M-SEARCH.
* SSDP client → bind(0.0.0.0:0), use an ephemeral port, send M-SEARCH, receive replies.

The client may still join the multicast group for debugging, but NOTIFY reception is optional.
*/
//! Client SSDP pour la découverte des devices UPnP

use super::{MAX_AGE, SSDP_MULTICAST_ADDR, SSDP_PORT};
use socket2::{Domain, Protocol, Socket, Type};
use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, trace, warn};

/// Événements SSDP intéressants pour un control point
#[derive(Debug, Clone)]
pub enum SsdpEvent {
    Alive {
        usn: String,
        nt: String,
        location: String,
        server: String,
        max_age: u32,
        from: SocketAddr,
    },
    ByeBye {
        usn: String,
        nt: String,
        from: SocketAddr,
    },
    SearchResponse {
        usn: String,
        st: String,
        location: String,
        server: String,
        max_age: u32,
        from: SocketAddr,
    },
}

/// Client SSDP pour envoyer des M-SEARCH et écouter les annonces
pub struct SsdpClient {
    socket: Arc<UdpSocket>,
}

impl SsdpClient {
    /// Crée un nouveau client SSDP
    pub fn new() -> std::io::Result<Self> {
        let addr = format!("{}:{}", SSDP_MULTICAST_ADDR, SSDP_PORT);

        let socket2 = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        socket2.set_reuse_address(true)?;

        #[cfg(windows)]
        {
            debug!("✅ SsdpClient SO_REUSEADDR enabled (Windows - SO_REUSEPORT not needed)");
        }

        let bind_addr: SocketAddr = "0.0.0.0:0".parse().unwrap();
        socket2.bind(&bind_addr.into())?;

        let socket: UdpSocket = socket2.into();
        socket.set_read_timeout(Some(Duration::from_secs(1)))?;
        socket.set_multicast_loop_v4(true)?; // utile en dev local

        for iface in get_if_addrs::get_if_addrs()? {
            if let std::net::IpAddr::V4(ipv4) = iface.ip() {
                if !ipv4.is_loopback() {
                    match socket.join_multicast_v4(&SSDP_MULTICAST_ADDR.parse().unwrap(), &ipv4) {
                        Ok(()) => {
                            debug!("SSDP: joined {} on {}", SSDP_MULTICAST_ADDR, ipv4);
                        }
                        Err(e) => {
                            warn!(
                                "SSDP: failed to join {} on {}: {}",
                                SSDP_MULTICAST_ADDR, ipv4, e
                            );
                        }
                    }
                }
            }
        }

        info!("✅ SSDP client ready on {}", addr);

        Ok(Self {
            socket: Arc::new(socket),
        })
    }

    /// Envoie un M-SEARCH pour un type donné
    pub fn send_msearch(&self, st: &str, mx: u32) -> std::io::Result<()> {
        let mx = mx.max(1); // MX doit être >= 1
        let msg = format!(
            "M-SEARCH * HTTP/1.1\r\n\
             HOST: {}:{}\r\n\
             MAN: \"ssdp:discover\"\r\n\
             MX: {}\r\n\
             ST: {}\r\n\
             USER-AGENT: PMOMusic SSDP Client\r\n\
             \r\n",
            SSDP_MULTICAST_ADDR, SSDP_PORT, mx, st
        );

        let addr: SocketAddr = format!("{}:{}", SSDP_MULTICAST_ADDR, SSDP_PORT)
            .parse()
            .unwrap();

        match self.socket.send_to(msg.as_bytes(), addr) {
            Ok(_) => {
                info!("📤 M-SEARCH sent (ST={}, MX={})", st, mx);
                debug!(
                    "📨 M-SEARCH payload\n<details>\n\n```\n{}\n```\n</details>\n",
                    msg
                );
                Ok(())
            }
            Err(e) => {
                warn!("❌ Failed to send M-SEARCH: {}", e);
                Err(e)
            }
        }
    }

    /// Boucle de réception bloquante pour traiter les événements SSDP
    pub fn run_event_loop<F>(&self, mut on_event: F) -> !
    where
        F: FnMut(SsdpEvent) + Send + 'static,
    {
        let socket = Arc::clone(&self.socket);
        let mut buf = [0u8; 8192];
        loop {
            match socket.recv_from(&mut buf) {
                Ok((n, from)) => {
                    let data = String::from_utf8_lossy(&buf[..n]);
                    if let Some(event) = parse_message(&data, from) {
                        debug!("📥 SSDP event from {}: {:?}", from, event);
                        on_event(event);
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    // Timeout, recommencer
                    continue;
                }
                Err(e) => {
                    warn!("❌ SSDP client read error: {}", e);
                }
            }
        }
    }
}

fn parse_message(data: &str, from: SocketAddr) -> Option<SsdpEvent> {
    let mut lines = data.lines();
    let first_line = lines.next()?.trim();
    let upper = first_line.to_ascii_uppercase();
    let headers = parse_headers(lines);

    let result = if upper.starts_with("NOTIFY ") {
        handle_notify(&headers, from)
    } else if upper.starts_with("HTTP/") && upper.contains(" 200 ") {
        handle_search_response(&headers, from)
    } else if upper.starts_with("M-SEARCH ") {
        // Another control point querying us; we are not a device, so we ignore.
        None
    } else {
        trace!("Unknown SSDP message type from {}: {}", from, first_line);
        None
    };

    if result.is_none() {
        trace!(
            "SSDP message from {} could not be parsed:\n{}",
            from,
            data
        );
    }

    result
}

fn handle_notify(headers: &HashMap<String, String>, from: SocketAddr) -> Option<SsdpEvent> {
    // Critical headers: NTS, NT, USN (required by UPnP spec)
    let nts = headers.get("NTS")?.to_ascii_lowercase();
    let nt = headers.get("NT")?.to_string();
    let usn = headers.get("USN")?.to_string();

    if nts == "ssdp:alive" {
        // LOCATION is required for alive notifications
        let location = match headers.get("LOCATION") {
            Some(loc) => loc.to_string(),
            None => {
                trace!(
                    "NOTIFY ssdp:alive from {} missing LOCATION header, ignoring",
                    from
                );
                return None;
            }
        };

        // Non-critical headers: SERVER, CACHE-CONTROL
        let server = headers
            .get("SERVER")
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                trace!("NOTIFY from {} has no SERVER header, using 'Unknown'", from);
                "Unknown".to_string()
            });
        let max_age = parse_max_age(headers.get("CACHE-CONTROL"));

        Some(SsdpEvent::Alive {
            usn,
            nt,
            location,
            server,
            max_age,
            from,
        })
    } else if nts == "ssdp:byebye" {
        Some(SsdpEvent::ByeBye { usn, nt, from })
    } else {
        trace!("Unknown NTS value from {}: {}", from, nts);
        None
    }
}

fn handle_search_response(
    headers: &HashMap<String, String>,
    from: SocketAddr,
) -> Option<SsdpEvent> {
    // Critical headers: ST, USN, LOCATION (required by UPnP spec)
    let st = match headers.get("ST") {
        Some(s) => s.to_string(),
        None => {
            trace!("M-SEARCH response from {} missing ST header, ignoring", from);
            return None;
        }
    };
    let usn = match headers.get("USN") {
        Some(u) => u.to_string(),
        None => {
            trace!(
                "M-SEARCH response from {} missing USN header, ignoring",
                from
            );
            return None;
        }
    };
    let location = match headers.get("LOCATION") {
        Some(loc) => loc.to_string(),
        None => {
            trace!(
                "M-SEARCH response from {} missing LOCATION header, ignoring",
                from
            );
            return None;
        }
    };

    // Non-critical headers: SERVER, CACHE-CONTROL
    let server = headers
        .get("SERVER")
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            trace!(
                "M-SEARCH response from {} has no SERVER header, using 'Unknown'",
                from
            );
            "Unknown".to_string()
        });
    let max_age = parse_max_age(headers.get("CACHE-CONTROL"));

    Some(SsdpEvent::SearchResponse {
        usn,
        st,
        location,
        server,
        max_age,
        from,
    })
}

fn parse_headers<'a, I>(lines: I) -> HashMap<String, String>
where
    I: Iterator<Item = &'a str>,
{
    let mut headers = HashMap::new();
    for line in lines {
        let line = line.trim();

        // Empty line marks end of headers
        if line.is_empty() {
            break;
        }

        // Split on first ':' only (values may contain ':')
        if let Some(colon_pos) = line.find(':') {
            let (name, value_with_colon) = line.split_at(colon_pos);
            let value = &value_with_colon[1..]; // Skip the ':'

            let name = name.trim().to_ascii_uppercase();
            let value = value.trim().to_string();

            // Skip empty header names or values
            if !name.is_empty() && !value.is_empty() {
                headers.insert(name, value);
            } else {
                trace!("Skipping malformed header: '{}'", line);
            }
        } else {
            // Line without ':' is invalid, skip it
            trace!("Skipping line without colon: '{}'", line);
        }
    }
    headers
}

fn parse_max_age(value: Option<&String>) -> u32 {
    if let Some(v) = value {
        // Try case-insensitive search for "max-age=<number>"
        let lower = v.to_ascii_lowercase();
        if let Some(idx) = lower.find("max-age") {
            // Extract everything after "max-age"
            let after_key = &v[idx + 7..];
            // Skip any whitespace and '=' characters
            let after_eq = after_key.trim_start().trim_start_matches('=').trim_start();
            // Try to parse the first sequence of digits
            let digits: String = after_eq
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if let Ok(age) = digits.parse::<u32>() {
                return age;
            }
        }
        trace!(
            "Could not parse max-age from CACHE-CONTROL: '{}', using default {}",
            v,
            MAX_AGE
        );
    }
    MAX_AGE
}
