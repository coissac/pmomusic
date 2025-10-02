use get_if_addrs::get_if_addrs;
use std::net::UdpSocket;

/// Devine l'adresse IP locale de la machine.
///
/// Cette fonction tente de déterminer l'adresse IP locale en créant une connexion UDP
/// vers un serveur DNS public (8.8.8.8). Cette technique permet d'identifier l'interface
/// réseau qui serait utilisée pour communiquer avec Internet.
///
/// # Fonctionnement
///
/// 1. Crée un socket UDP lié à `0.0.0.0:0` (n'importe quelle interface, port aléatoire)
/// 2. Tente une connexion (non effective pour UDP) vers `8.8.8.8:80`
/// 3. Récupère l'adresse IP locale du socket
/// 4. En cas d'échec à n'importe quelle étape, retourne `127.0.0.1`
///
/// # Returns
///
/// Retourne l'adresse IP locale sous forme de `String`, ou `"127.0.0.1"` en cas d'erreur.
///
/// # Examples
///
/// ```
/// let ip = guess_local_ip();
/// println!("IP locale détectée: {}", ip);
/// // Affiche par exemple: "IP locale détectée: 192.168.1.42"
/// ```
///
/// # Note
///
/// Cette méthode ne crée pas de véritable connexion réseau (UDP est sans connexion),
/// elle demande simplement au système d'exploitation quelle interface serait utilisée
/// pour joindre l'adresse cible.
pub fn guess_local_ip() -> String {
    match UdpSocket::bind("0.0.0.0:0") {
        Ok(socket) => {
            if socket.connect("8.8.8.8:80").is_ok() {
                if let Ok(local_addr) = socket.local_addr() {
                    return local_addr.ip().to_string();
                }
            }
            "127.0.0.1".to_string()
        }
        Err(_) => "127.0.0.1".to_string(),
    }
}

/// Liste toutes les adresses IP non-loopback des interfaces réseau.
///
/// Parcourt toutes les interfaces réseau de la machine et collecte leurs adresses IPv4,
/// en excluant les adresses de loopback (127.0.0.1).
///
/// # Returns
///
/// Retourne une `HashMap` où :
/// - **Clé** : nom de l'interface réseau (ex: `"eth0"`, `"wlan0"`, `"en0"`)
/// - **Valeur** : vecteur des adresses IP (format String) associées à cette interface
///
/// En cas d'erreur lors de la récupération des interfaces, retourne une HashMap
/// contenant une entrée `"error"` avec un message d'erreur.
///
/// # Examples
///
/// ```
/// let ips = list_all_ips();
/// for (interface, addresses) in ips {
///     println!("Interface {}: {:?}", interface, addresses);
/// }
/// // Affiche par exemple:
/// // Interface eth0: ["192.168.1.42"]
/// // Interface wlan0: ["10.0.0.15"]
/// ```
///
/// # Note
///
/// - Seules les adresses IPv4 sont retournées
/// - Les adresses de loopback (127.x.x.x) sont filtrées
/// - Les adresses IPv6 sont ignorées
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::IpAddr;

    #[test]
    fn test_guess_local_ip_returns_valid_ip() {
        let ip = guess_local_ip();
        
        // Vérifie que le résultat est parsable comme une IP
        assert!(ip.parse::<IpAddr>().is_ok(), "Should return a valid IP address");
    }

    #[test]
    fn test_guess_local_ip_not_empty() {
        let ip = guess_local_ip();
        
        assert!(!ip.is_empty(), "IP should not be empty");
    }

    #[test]
    fn test_guess_local_ip_is_ipv4() {
        let ip = guess_local_ip();
        
        if let Ok(parsed_ip) = ip.parse::<IpAddr>() {
            assert!(parsed_ip.is_ipv4(), "Should return an IPv4 address");
        }
    }

    #[test]
    fn test_guess_local_ip_fallback_is_localhost() {
        // Ce test vérifie que si aucune IP n'est trouvée, on retourne 127.0.0.1
        // (difficile à tester sans mocker, mais on vérifie la cohérence)
        let ip = guess_local_ip();
        let parsed = ip.parse::<IpAddr>().unwrap();
        
        // L'IP doit être soit locale (127.0.0.1) soit une IP privée valide
        assert!(
            parsed.is_loopback() || is_private_ip(&ip),
            "IP should be either loopback or private"
        );
    }

    #[test]
    fn test_list_all_ips_no_loopback() {
        let ips = list_all_ips();
        
        // Vérifie qu'aucune adresse de loopback n'est présente
        for (_, addresses) in ips.iter() {
            for addr in addresses {
                if let Ok(parsed_ip) = addr.parse::<IpAddr>() {
                    assert!(
                        !parsed_ip.is_loopback(),
                        "Loopback addresses should be filtered out"
                    );
                }
            }
        }
    }

    #[test]
    fn test_list_all_ips_only_ipv4() {
        let ips = list_all_ips();
        
        // Vérifie que seules des adresses IPv4 sont retournées
        for (iface_name, addresses) in ips.iter() {
            if iface_name == "error" {
                continue; // Skip error entries
            }
            
            for addr in addresses {
                if let Ok(parsed_ip) = addr.parse::<IpAddr>() {
                    assert!(
                        parsed_ip.is_ipv4(),
                        "Only IPv4 addresses should be returned"
                    );
                }
            }
        }
    }

    #[test]
    fn test_list_all_ips_valid_format() {
        let ips = list_all_ips();
        
        // Vérifie que toutes les IPs sont dans un format valide
        for (iface_name, addresses) in ips.iter() {
            if iface_name == "error" {
                continue;
            }
            
            for addr in addresses {
                assert!(
                    addr.parse::<IpAddr>().is_ok(),
                    "Each IP should be in valid format: {}",
                    addr
                );
            }
        }
    }

    #[test]
    fn test_list_all_ips_interface_names_not_empty() {
        let ips = list_all_ips();
        
        // Vérifie que les noms d'interface ne sont pas vides
        for (iface_name, _) in ips.iter() {
            assert!(!iface_name.is_empty(), "Interface names should not be empty");
        }
    }

    #[test]
    fn test_list_all_ips_no_duplicate_ips_per_interface() {
        let ips = list_all_ips();
        
        // Vérifie qu'il n'y a pas de doublons par interface
        for (iface_name, addresses) in ips.iter() {
            if iface_name == "error" {
                continue;
            }
            
            let unique_addresses: std::collections::HashSet<_> = addresses.iter().collect();
            assert_eq!(
                addresses.len(),
                unique_addresses.len(),
                "No duplicate IPs should exist for interface {}",
                iface_name
            );
        }
    }

    // Fonction helper pour les tests
    fn is_private_ip(ip_str: &str) -> bool {
        if let Ok(ip) = ip_str.parse::<IpAddr>() {
            match ip {
                IpAddr::V4(ipv4) => {
                    let octets = ipv4.octets();
                    // Plages privées: 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
                    octets[0] == 10
                        || (octets[0] == 172 && octets[1] >= 16 && octets[1] <= 31)
                        || (octets[0] == 192 && octets[1] == 168)
                }
                IpAddr::V6(_) => false,
            }
        } else {
            false
        }
    }

    #[test]
    fn test_helper_is_private_ip() {
        // Tests pour la fonction helper
        assert!(is_private_ip("10.0.0.1"));
        assert!(is_private_ip("172.16.0.1"));
        assert!(is_private_ip("192.168.1.1"));
        assert!(!is_private_ip("8.8.8.8"));
        assert!(!is_private_ip("127.0.0.1")); // loopback n'est pas "privé" au sens réseau local
    }
}