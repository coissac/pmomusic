use get_if_addrs::get_if_addrs;
use std::net::UdpSocket;

pub fn guess_local_ip() -> String {
    // On tente de deviner l'IP locale
    match UdpSocket::bind("0.0.0.0:0") {
        Ok(socket) => {
            if socket.connect("8.8.8.8:80").is_ok() {
                if let Ok(local_addr) = socket.local_addr() {
                    return local_addr.ip().to_string();
                }
            }
            // Si erreur sur connect ou récupération de l'adresse
            "127.0.0.1".to_string()
        }
        Err(_) => "127.0.0.1".to_string(), // Si bind échoue
    }
}

fn list_all_ips() -> std::collections::HashMap<String, Vec<String>> {
    let mut result = std::collections::HashMap::new();

    if let Ok(interfaces) = get_if_addrs() {
        for iface in interfaces {
            let ip = iface.ip();
            if ip.is_loopback() {
                continue;
            }
            if ip.is_ipv4() {
                result
                    .entry(iface.name)
                    .or_insert_with(Vec::new)
                    .push(ip.to_string());
            }
        }
    } else {
        result.insert(
            "error".to_string(),
            vec!["Failed to get interfaces".to_string()],
        );
    }

    result
}
