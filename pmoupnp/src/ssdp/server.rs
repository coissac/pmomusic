//! Serveur SSDP

use super::{MAX_AGE, SSDP_MULTICAST_ADDR, SSDP_PORT, SsdpDevice};
use socket2::{Domain, Protocol, Socket, Type};
use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tracing::{debug, info, warn};

/// Serveur SSDP gérant les annonces et découvertes
pub struct SsdpServer {
    /// Devices enregistrés (UUID -> Device)
    devices: Arc<RwLock<HashMap<String, SsdpDevice>>>,

    /// Socket UDP pour SSDP
    socket: Option<Arc<UdpSocket>>,
}

impl SsdpServer {
    /// Crée un nouveau serveur SSDP
    pub fn new() -> Self {
        Self {
            devices: Arc::new(RwLock::new(HashMap::new())),
            socket: None,
        }
    }

    /// Démarre le serveur SSDP
    ///
    /// # Returns
    ///
    /// `Ok(())` si le démarrage a réussi, `Err` sinon
    pub fn start(&mut self) -> std::io::Result<()> {
        let addr = format!("{}:{}", SSDP_MULTICAST_ADDR, SSDP_PORT);

        // Créer le socket avec socket2 pour permettre la réutilisation du port
        // Ceci est essentiel pour que plusieurs clients/serveurs UPnP puissent coexister
        let socket2 = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;

        // SO_REUSEADDR : permet à plusieurs sockets de bind sur le même port
        // Essentiel sur toutes les plateformes pour le multicast
        socket2.set_reuse_address(true)?;

        // SO_REUSEPORT : nécessaire sur Unix (macOS/Linux/BSD) pour que plusieurs processus
        // puissent recevoir du trafic multicast sur le même port.
        // Windows n'a pas besoin de SO_REUSEPORT - SO_REUSEADDR suffit.
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let fd = socket2.as_raw_fd();
            let optval: libc::c_int = 1;
            unsafe {
                let result = libc::setsockopt(
                    fd,
                    libc::SOL_SOCKET,
                    libc::SO_REUSEPORT,
                    &optval as *const _ as *const libc::c_void,
                    std::mem::size_of_val(&optval) as libc::socklen_t,
                );
                if result != 0 {
                    return Err(std::io::Error::last_os_error());
                }
            }
            debug!("✅ SO_REUSEPORT enabled (Unix)");
        }

        #[cfg(windows)]
        {
            debug!("✅ SO_REUSEADDR enabled (Windows - SO_REUSEPORT not needed)");
        }

        // Bind sur 0.0.0.0:1900
        let bind_addr: SocketAddr = format!("0.0.0.0:{}", SSDP_PORT).parse().unwrap();
        socket2.bind(&bind_addr.into())?;

        // Configurer l'interface de sortie multicast sur l'IP principale.
        // Sur macOS, sans cela le kernel peut router les paquets multicast
        // via une interface bridge/VM qui n'a pas de route, causant
        // EHOSTUNREACH (errno 65).
        let local_ip: std::net::Ipv4Addr = pmoutils::guess_local_ip()
            .parse()
            .unwrap_or("127.0.0.1".parse().unwrap());
        socket2.set_multicast_if_v4(&local_ip)?;
        debug!(
            "SSDP server: multicast outgoing interface set to {}",
            local_ip
        );

        // Convertir en UdpSocket standard
        let socket: UdpSocket = socket2.into();

        // Rejoindre le groupe multicast via INADDR_ANY (l'OS choisit l'interface)
        socket.join_multicast_v4(
            &SSDP_MULTICAST_ADDR.parse().unwrap(),
            &"0.0.0.0".parse().unwrap(),
        )?;

        socket.set_read_timeout(Some(Duration::from_secs(1)))?;
        socket.set_multicast_loop_v4(true)?; // utile en dev local

        let socket = Arc::new(socket);
        self.socket = Some(socket.clone());

        info!("✅ SSDP server started on {}", addr);

        // Lancer les goroutines d'annonces périodiques et d'écoute M-SEARCH
        self.start_periodic_announcements(socket.clone());
        self.start_msearch_listener(socket.clone());

        Ok(())
    }

    /// Ajoute un device et envoie un alive initial
    pub fn add_device(&self, device: SsdpDevice) {
        let uuid = device.uuid.clone();
        let mut devices = self.devices.write().unwrap();
        devices.insert(uuid.clone(), device.clone());
        drop(devices);

        info!(
            "🆕 SSDP device registered: {} ({} NTs)",
            uuid,
            device.get_notification_types().len()
        );
        debug!(
            "🆕 SSDP device notification types for {}: {:?}",
            uuid,
            device.get_notification_types()
        );

        // Envoyer alive pour tous les NTs
        if let Some(ref socket) = self.socket {
            let nts = device.get_notification_types();
            for nt in nts.iter() {
                Self::send_alive(socket, &device, nt, false);
                // Petit délai pour éviter de saturer le buffer UDP sur macOS
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }

    /// Supprime un device et envoie un byebye
    pub fn remove_device(&self, uuid: &str) {
        let mut devices = self.devices.write().unwrap();
        if let Some(device) = devices.remove(uuid) {
            drop(devices);

            info!(
                "🗑️ SSDP device removed: {} ({} NTs)",
                uuid,
                device.get_notification_types().len()
            );

            // Envoyer byebye pour tous les NTs
            if let Some(ref socket) = self.socket {
                for nt in device.get_notification_types() {
                    self.send_byebye(socket, &device, nt);
                }
            }
        }
    }

    /// Envoie un NOTIFY alive
    fn send_alive(socket: &UdpSocket, device: &SsdpDevice, nt: &str, is_periodic: bool) {
        let usn = if nt.starts_with("uuid:") {
            format!("{}", nt)
        } else {
            format!("uuid:{}::{}", device.uuid, nt)
        };

        let msg = format!(
            "NOTIFY * HTTP/1.1\r\n\
             HOST: {}:{}\r\n\
             CACHE-CONTROL: max-age={}\r\n\
             LOCATION: {}\r\n\
             NT: {}\r\n\
             NTS: ssdp:alive\r\n\
             SERVER: {}\r\n\
             USN: {}\r\n\
             \r\n",
            SSDP_MULTICAST_ADDR, SSDP_PORT, MAX_AGE, device.location, nt, device.server, usn
        );

        let addr: SocketAddr = format!("{}:{}", SSDP_MULTICAST_ADDR, SSDP_PORT)
            .parse()
            .unwrap();

        match socket.send_to(msg.as_bytes(), addr) {
            Ok(_) => {
                let label = if is_periodic { " (periodic)" } else { "" };
                info!("✅ NOTIFY alive{}: {} (NT={})", label, usn, nt);
                debug!(
                    "📣 NOTIFY alive{} payload\n<details>\n\n```\n{}\n```\n</details>\n",
                    label, msg
                );
            }

            Err(e) => {
                let label = if is_periodic { "periodic " } else { "" };
                warn!("❌ Failed to send {}NOTIFY alive for {}: {}", label, usn, e);
            }
        }
    }

    /// Envoie un NOTIFY byebye
    fn send_byebye(&self, socket: &UdpSocket, device: &SsdpDevice, nt: &str) {
        let usn = if nt.starts_with("uuid:") {
            format!("{}", nt)
        } else {
            format!("uuid:{}::{}", device.uuid, nt)
        };

        let msg = format!(
            "NOTIFY * HTTP/1.1\r\n\
             HOST: {}:{}\r\n\
             NT: {}\r\n\
             NTS: ssdp:byebye\r\n\
             USN: {}\r\n\
             \r\n",
            SSDP_MULTICAST_ADDR, SSDP_PORT, nt, usn
        );

        let addr: SocketAddr = format!("{}:{}", SSDP_MULTICAST_ADDR, SSDP_PORT)
            .parse()
            .unwrap();

        match socket.send_to(msg.as_bytes(), addr) {
            Ok(_) => {
                info!("👋 NOTIFY byebye: {} (NT={})", usn, nt);
                debug!(
                    "📣 NOTIFY byebye payload\n<details>\n\n```\n{}\n```\n</details>\n",
                    msg
                );
            }
            Err(e) => warn!("❌ Failed to send NOTIFY byebye for {}: {}", usn, e),
        }
    }

    /// Démarre les annonces périodiques (toutes les MAX_AGE/2 secondes)
    fn start_periodic_announcements(&self, socket: Arc<UdpSocket>) {
        let devices = Arc::clone(&self.devices);
        let period = Duration::from_secs((MAX_AGE / 2) as u64);

        std::thread::spawn(move || {
            loop {
                debug!("⏰ SSDP periodic announcement tick");
                std::thread::sleep(period);

                // Clone la liste des devices pour libérer le lock rapidement
                let devices_snapshot: Vec<SsdpDevice> = {
                    let devices = devices.read().unwrap();
                    devices.values().cloned().collect()
                };
                for device in &devices_snapshot {
                    for nt in device.get_notification_types() {
                        Self::send_alive(&socket, device, nt, true);
                    }
                }
            }
        });
    }

    /// Démarre l'écoute des M-SEARCH
    fn start_msearch_listener(&self, socket: Arc<UdpSocket>) {
        let devices = Arc::clone(&self.devices);

        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match socket.recv_from(&mut buf) {
                    Ok((n, src)) => {
                        let data = String::from_utf8_lossy(&buf[..n]);
                        if data.starts_with("M-SEARCH") {
                            debug!(
                                "🔍 M-SEARCH received from {}\n<details>\n\n```\n{}\n```\n</details>\n",
                                src, data
                            );
                            if let Some(st) = Self::parse_st(&data) {
                                // Clone la liste des devices pour libérer le lock rapidement
                                let devices_snapshot: Vec<SsdpDevice> = {
                                    let devices = devices.read().unwrap();
                                    devices.values().cloned().collect()
                                };
                                for device in &devices_snapshot {
                                    Self::handle_msearch(&socket, &src, &st, device);
                                }
                            }
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        // Timeout, continuer
                        continue;
                    }
                    Err(e) => {
                        warn!("❌ SSDP read error: {}", e);
                    }
                }
            }
        });
    }

    /// Parse le champ ST d'un M-SEARCH
    fn parse_st(data: &str) -> Option<String> {
        for line in data.lines() {
            if line.to_uppercase().starts_with("ST:") {
                let st = line[3..].trim().to_string();
                info!("✅ M-SEARCH received with ST={}", st);
                return Some(st);
            }
        }
        None
    }

    /// Répond à un M-SEARCH
    fn handle_msearch(socket: &UdpSocket, src: &SocketAddr, st: &str, device: &SsdpDevice) {
        let mut nts = Vec::new();

        if st == "ssdp:all" {
            nts.extend(device.get_notification_types().iter().cloned());
        } else if device.get_notification_types().contains(&st.to_string()) {
            nts.push(st.to_string());
        } else {
            return; // Pas de match
        }

        for nt in nts {
            let usn = if nt.starts_with("uuid:") {
                format!("{}", nt)
            } else {
                format!("uuid:{}::{}", device.uuid, nt)
            };

            let date = chrono::Utc::now().format("%a, %d %b %Y %H:%M:%S GMT");

            let resp = format!(
                "HTTP/1.1 200 OK\r\n\
                 CACHE-CONTROL: max-age={}\r\n\
                 DATE: {}\r\n\
                 EXT:\r\n\
                 LOCATION: {}\r\n\
                 SERVER: {}\r\n\
                 ST: {}\r\n\
                 USN: {}\r\n\
                 \r\n",
                MAX_AGE, date, device.location, device.server, nt, usn
            );
            match socket.send_to(resp.as_bytes(), src) {
                Ok(_) => {
                    debug!(
                        "📡 M-SEARCH response sent to {} with ST={}\n\n### payload\n\n<details>\n\n```\n{}\n```\n</details>\n",
                        src, nt, resp
                    );
                }
                Err(e) => warn!("❌ Failed to send M-SEARCH response to {}: {}", src, e),
            }
        }
    }
}

impl Default for SsdpServer {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for SsdpServer {
    fn drop(&mut self) {
        // Envoyer byebye pour tous les devices
        if let Some(ref socket) = self.socket {
            info!("✅ Shutting down SSDP server, sending byebye for all devices");
            let devices = self.devices.read().unwrap();
            for device in devices.values() {
                for nt in device.get_notification_types() {
                    self.send_byebye(socket, device, nt);
                }
            }
        }
    }
}
