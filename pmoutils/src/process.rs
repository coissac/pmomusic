use netstat2::{AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo, get_sockets_info};
use sysinfo::{Pid, System};

/// Informations sur un processus utilisant un port réseau.
#[derive(Debug, Clone)]
pub struct ProcessPortInfo {
    pub pid: u32,
    pub process_name: String,
    pub owner: String,
    pub port: u16,
}

/// Protocole de transport utilisé pour la recherche.
#[derive(Debug, Clone, Copy)]
pub enum TransportProtocol {
    Tcp,
    Udp,
}

/// Tente de trouver le processus qui écoute sur `port` pour le protocole donné.
///
/// Retourne `Some(ProcessPortInfo)` si un processus a pu être identifié, sinon `None`.
pub fn find_process_using_port(port: u16, protocol: TransportProtocol) -> Option<ProcessPortInfo> {
    let proto_flag = match protocol {
        TransportProtocol::Tcp => ProtocolFlags::TCP,
        TransportProtocol::Udp => ProtocolFlags::UDP,
    };

    let sockets = get_sockets_info(
        AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6,
        proto_flag,
    )
    .ok()?;

    // Préparer l'inspection des processus.
    let mut system = System::new_all();
    system.refresh_all();

    for socket in sockets {
        match socket.protocol_socket_info {
            ProtocolSocketInfo::Tcp(ref tcp_info)
                if matches!(protocol, TransportProtocol::Tcp) && tcp_info.local_port == port =>
            {
                if let Some(info) =
                    build_process_info(&mut system, port, socket.associated_pids.first())
                {
                    return Some(info);
                }
            }
            ProtocolSocketInfo::Udp(ref udp_info)
                if matches!(protocol, TransportProtocol::Udp) && udp_info.local_port == port =>
            {
                if let Some(info) =
                    build_process_info(&mut system, port, socket.associated_pids.first())
                {
                    return Some(info);
                }
            }
            _ => continue,
        }
    }

    None
}

fn build_process_info(
    system: &mut System,
    port: u16,
    pid_opt: Option<&u32>,
) -> Option<ProcessPortInfo> {
    let pid = *pid_opt?;
    let process = system.process(Pid::from_u32(pid))?;
    let process_name = process.name().to_string();

    let owner = process
        .user_id()
        .and_then(|uid| {
            users::get_user_by_uid(**uid).map(|user| user.name().to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "unknown".to_string());

    Some(ProcessPortInfo {
        pid,
        process_name,
        owner,
        port,
    })
}
