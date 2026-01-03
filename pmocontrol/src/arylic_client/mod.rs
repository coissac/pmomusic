use std::{
    io::{Read, Write},
    net::{Shutdown, TcpStream, ToSocketAddrs},
    sync::{Mutex, OnceLock},
    thread,
    time::{Duration, Instant},
};

use tracing::{debug, warn};

use crate::errors::ControlPointError;

pub const ARYLIC_TCP_PORT: u16 = 8899;
pub const DEFAULT_TIMEOUT_SECS: u64 = 3;

const PACKET_HEADER: [u8; 4] = [0x18, 0x96, 0x18, 0x20];
const RESERVED_BYTES: [u8; 8] = [0; 8];
const MAX_RESPONSE_ATTEMPTS: usize = 8;

// Garde global pour respecter le délai de 200ms entre commandes
static LAST_COMMAND_TIME: OnceLock<Mutex<Instant>> = OnceLock::new();

fn last_command_time() -> &'static Mutex<Instant> {
    LAST_COMMAND_TIME.get_or_init(|| Mutex::new(Instant::now()))
}

/// Mode d’attente de réponse pour une commande TCP Arylic.
enum ResponseMode<'a> {
    /// On n’attend aucune réponse (fire-and-forget).
    None,
    /// On attend une réponse, mais si la lecture échoue immédiatement, on traite comme succès.
    Optional(&'a [&'a str]),
    /// On attend une réponse, et l’absence de réponse est une erreur.
    Required(&'a [&'a str]),
}

fn send_command_with_mode(
    host: &str,
    port: u16,
    timeout: Duration,
    payload: &str,
    mode: ResponseMode<'_>,
) -> Result<Option<String>, ControlPointError> {
    let mut stream = connect(host, port, timeout)?;
    let packet = encode_packet(payload);

    stream.write_all(&packet).map_err(|_| {
        ControlPointError::ArilycTcpError(format!(
            "Failed to write Arylic TCP packet for {}: {}",
            host, payload
        ))
    })?;

    stream.flush().map_err(|_| {
        ControlPointError::ArilycTcpError(format!(
            "Failed to flush Arylic TCP stream for {} (command {})",
            host, payload
        ))
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
        ResponseMode::Required(expected) => {
            read_expected_response(&mut stream, host, payload, expected).map(Some)
        }
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

            Err(ControlPointError::ArilycTcpError(format!(
                "No expected response for optional command {} on {}",
                payload, host
            )))
        }
    }
}

fn read_expected_response(
    stream: &mut TcpStream,
    host: &str,
    payload: &str,
    expected: &[&str],
) -> Result<String, ControlPointError> {
    for _ in 0..MAX_RESPONSE_ATTEMPTS {
        let response = match read_packet(stream) {
            Ok(resp) => resp,
            Err(err) => {
                return Err(ControlPointError::ArilycTcpError(format!(
                    "Failed to read Arylic TCP response for {} (command {}): {}",
                    host, payload, err
                )));
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

    Err(ControlPointError::ArilycTcpError(format!(
        "No expected response for command {} on {}",
        payload, host
    )))
}

pub fn send_command_required(
    host: &str,
    port: u16,
    timeout: Duration,
    payload: &str,
    expected: &[&str],
) -> Result<String, ControlPointError> {
    match send_command_with_mode(
        host,
        port,
        timeout,
        payload,
        ResponseMode::Required(expected),
    )? {
        Some(s) => Ok(s),
        None => Err(ControlPointError::ArilycTcpError(format!(
            "Arylic TCP: no response payload for required command {}",
            payload
        ))),
    }
}

pub fn send_command_optional(
    host: &str,
    port: u16,
    timeout: Duration,
    payload: &str,
    expected: &[&str],
) -> Result<Option<String>, ControlPointError> {
    send_command_with_mode(
        host,
        port,
        timeout,
        payload,
        ResponseMode::Optional(expected),
    )
}

pub fn send_command_no_response(
    host: &str,
    port: u16,
    timeout: Duration,
    payload: &str,
) -> Result<(), ControlPointError> {
    send_command_with_mode(host, port, timeout, payload, ResponseMode::None).map(|_| ())
}

fn connect(host: &str, port: u16, timeout: Duration) -> Result<TcpStream, ControlPointError> {
    if let Ok(mut last_time) = last_command_time().lock() {
        let elapsed = last_time.elapsed();
        if elapsed < Duration::from_millis(200) {
            let wait = Duration::from_millis(200) - elapsed;
            debug!(
                "Waiting {:?} before sending command to respect 200ms interval",
                wait
            );
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
    for addr in address.to_socket_addrs().map_err(|_| {
        ControlPointError::ArilycTcpError(format!("Failed to resolve {}:{}", host, port))
    })? {
        match TcpStream::connect_timeout(&addr, timeout) {
            Ok(stream) => {
                stream
                    .set_read_timeout(Some(timeout))
                    .and_then(|_| stream.set_write_timeout(Some(timeout)))
                    .map_err(|_| {
                        ControlPointError::ArilycTcpError(format!(
                            "Failed to set socket timeouts for {}",
                            address
                        ))
                    })?;

                return Ok(stream);
            }
            Err(err) => {
                last_err = Some((addr, err));
            }
        }
    }

    match last_err {
        Some((addr, err)) => Err(ControlPointError::ArilycTcpError(format!(
            "Failed to connect to {} via {}: {}",
            host, addr, err
        ))),
        None => Err(ControlPointError::ArilycTcpError(format!(
            "No socket addresses resolved for {}",
            address
        ))),
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

fn read_packet(stream: &mut TcpStream) -> Result<String, ControlPointError> {
    let mut header = [0u8; 4];
    stream
        .read_exact(&mut header)
        .map_err(|e| ControlPointError::ArilycTcpError(format!("{}", e)))?;
    if header != PACKET_HEADER {
        return Err(ControlPointError::ArilycTcpError(format!(
            "Invalid Arylic packet header: {:x?}",
            header
        )));
    }

    let mut len_buf = [0u8; 4];
    stream
        .read_exact(&mut len_buf)
        .map_err(|e| ControlPointError::ArilycTcpError(format!("{}", e)))?;
    let len = u32::from_le_bytes(len_buf) as usize;

    let mut checksum_buf = [0u8; 4];
    stream
        .read_exact(&mut checksum_buf)
        .map_err(|e| ControlPointError::ArilycTcpError(format!("{}", e)))?;
    let expected_checksum = u32::from_le_bytes(checksum_buf);

    let mut reserved = [0u8; 8];
    stream
        .read_exact(&mut reserved)
        .map_err(|e| ControlPointError::ArilycTcpError(format!("{}", e)))?;

    let mut payload = vec![0u8; len];
    stream
        .read_exact(&mut payload)
        .map_err(|e| ControlPointError::ArilycTcpError(format!("{}", e)))?;

    let actual_checksum = payload.iter().fold(0u32, |acc, b| acc + (*b as u32));
    if actual_checksum != expected_checksum {
        warn!(
            "Arylic payload checksum mismatch: expected={} actual={}",
            expected_checksum, actual_checksum
        );
    }

    Ok(String::from_utf8(payload)
        .map_err(|e| ControlPointError::ArilycTcpError(format!("{}", e)))?)
}
