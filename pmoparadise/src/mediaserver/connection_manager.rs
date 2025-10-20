//! ConnectionManager service implementation

use pmoupnp::actions::Action;
use pmoupnp::services::Service;
use pmoupnp::state_variables::StateVariable;
use std::sync::Arc;

/// Create a ConnectionManager service
///
/// The ConnectionManager service provides information about supported
/// protocols and connections.
pub fn create_connection_manager_service() -> Service {
    let mut service = Service::new("ConnectionManager".to_string());
    service.set_service_type("urn:schemas-upnp-org:service:ConnectionManager:1".to_string());
    service.set_service_id("urn:upnp-org:serviceId:ConnectionManager".to_string());

    // State variables
    let source_protocol_info =
        StateVariable::new("SourceProtocolInfo".to_string(), "string".to_string())
            .with_send_events(true)
            .with_default_value(get_protocol_info());

    let sink_protocol_info =
        StateVariable::new("SinkProtocolInfo".to_string(), "string".to_string())
            .with_send_events(true)
            .with_default_value("".to_string());

    let current_connection_ids =
        StateVariable::new("CurrentConnectionIDs".to_string(), "string".to_string())
            .with_send_events(true)
            .with_default_value("0".to_string());

    service.add_state_variable(Arc::new(source_protocol_info));
    service.add_state_variable(Arc::new(sink_protocol_info));
    service.add_state_variable(Arc::new(current_connection_ids));

    // GetProtocolInfo action
    let mut get_protocol_info = Action::new("GetProtocolInfo".to_string());
    get_protocol_info.add_output_argument("Source".to_string(), "SourceProtocolInfo".to_string());
    get_protocol_info.add_output_argument("Sink".to_string(), "SinkProtocolInfo".to_string());
    service.add_action(Arc::new(get_protocol_info));

    // GetCurrentConnectionIDs action
    let mut get_connection_ids = Action::new("GetCurrentConnectionIDs".to_string());
    get_connection_ids.add_output_argument(
        "ConnectionIDs".to_string(),
        "CurrentConnectionIDs".to_string(),
    );
    service.add_action(Arc::new(get_connection_ids));

    // GetCurrentConnectionInfo action
    let mut get_connection_info = Action::new("GetCurrentConnectionInfo".to_string());
    get_connection_info.add_input_argument(
        "ConnectionID".to_string(),
        "A_ARG_TYPE_ConnectionID".to_string(),
    );
    get_connection_info.add_output_argument("RcsID".to_string(), "A_ARG_TYPE_RcsID".to_string());
    get_connection_info.add_output_argument(
        "AVTransportID".to_string(),
        "A_ARG_TYPE_AVTransportID".to_string(),
    );
    get_connection_info.add_output_argument(
        "ProtocolInfo".to_string(),
        "A_ARG_TYPE_ProtocolInfo".to_string(),
    );
    get_connection_info.add_output_argument(
        "PeerConnectionManager".to_string(),
        "A_ARG_TYPE_ConnectionManager".to_string(),
    );
    get_connection_info.add_output_argument(
        "PeerConnectionID".to_string(),
        "A_ARG_TYPE_ConnectionID".to_string(),
    );
    get_connection_info
        .add_output_argument("Direction".to_string(), "A_ARG_TYPE_Direction".to_string());
    get_connection_info.add_output_argument(
        "Status".to_string(),
        "A_ARG_TYPE_ConnectionStatus".to_string(),
    );
    service.add_action(Arc::new(get_connection_info));

    // Additional state variables for arguments
    service.add_state_variable(Arc::new(StateVariable::new(
        "A_ARG_TYPE_ConnectionID".to_string(),
        "i4".to_string(),
    )));
    service.add_state_variable(Arc::new(StateVariable::new(
        "A_ARG_TYPE_RcsID".to_string(),
        "i4".to_string(),
    )));
    service.add_state_variable(Arc::new(StateVariable::new(
        "A_ARG_TYPE_AVTransportID".to_string(),
        "i4".to_string(),
    )));
    service.add_state_variable(Arc::new(StateVariable::new(
        "A_ARG_TYPE_ProtocolInfo".to_string(),
        "string".to_string(),
    )));
    service.add_state_variable(Arc::new(StateVariable::new(
        "A_ARG_TYPE_ConnectionManager".to_string(),
        "string".to_string(),
    )));
    service.add_state_variable(Arc::new(
        StateVariable::new("A_ARG_TYPE_Direction".to_string(), "string".to_string())
            .with_allowed_values(vec!["Input".to_string(), "Output".to_string()]),
    ));
    service.add_state_variable(Arc::new(
        StateVariable::new(
            "A_ARG_TYPE_ConnectionStatus".to_string(),
            "string".to_string(),
        )
        .with_allowed_values(vec![
            "OK".to_string(),
            "ContentFormatMismatch".to_string(),
            "InsufficientBandwidth".to_string(),
            "UnreliableChannel".to_string(),
            "Unknown".to_string(),
        ]),
    ));

    service
}

/// Get the protocol info string
///
/// Lists all supported protocols for Radio Paradise streaming.
fn get_protocol_info() -> String {
    vec![
        // HTTP FLAC
        "http-get:*:audio/flac:*",
        "http-get:*:audio/x-flac:*",
        // HTTP AAC
        "http-get:*:audio/aac:*",
        "http-get:*:audio/aacp:*",
        "http-get:*:audio/x-aac:*",
        // HTTP MP3
        "http-get:*:audio/mpeg:*",
        "http-get:*:audio/mp3:*",
        "http-get:*:audio/x-mp3:*",
    ]
    .join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_connection_manager() {
        let service = create_connection_manager_service();
        assert_eq!(
            service.service_type(),
            "urn:schemas-upnp-org:service:ConnectionManager:1"
        );
        assert_eq!(
            service.service_id(),
            "urn:upnp-org:serviceId:ConnectionManager"
        );
    }

    #[test]
    fn test_protocol_info() {
        let info = get_protocol_info();
        assert!(info.contains("audio/flac"));
        assert!(info.contains("audio/aac"));
        assert!(info.contains("audio/mpeg"));
    }
}
