use thiserror::Error;

#[derive(Error, Debug)]
pub enum ControlPointError {
    // Utiliser par les DeviceItems dans leur conversion vers un media renderer
    #[error("{0} is not a MediaRenderer")]
    IsNotAMediaRender(String),
    #[error("{0} is not a MediaServer")]
    IsNotAMediaServer(String),
    #[error("Cannot build MusicRendererBackend for {0}")]
    MusicRendererBackendBuild(String),
    #[error("{0}")]
    ParsingError(String),
    #[error("OpenHome Error: {0}")]
    OpenHomeError(String),
    #[error("OpenHome Product source list does not expose a Playlist entry")]
    OpenHomeNoPlaylistEntry(),
    #[error("MusicRendererBackend {0} exposes no OpenHome services")]
    OpenHomeNotAValidDevice(String),
    #[error("UpnpError Error: {0}")]
    UpnpError(String),
    #[error("MusicRenderer operation '{0}' is not supported by backend '{1}'")]
    UpnpOperationNotSupported(String, String),
    #[error("Missing {0} element in SOAP body")]
    UpnpMissingReturnValue(String),
    #[error("Invalid {0} value: {1}")]
    UpnpBadReturnValue(String, String),
    #[error("Soap Error: Upnp action call {0}")]
    SoapAction(String),
    #[error("{0} returned UPnP error {1}: {2} (HTTP status {3})")]
    SoapUpnpParseError(String, u32, String, u32),
    #[error("{0} failed with HTTP status {1} and body: {2}")]
    SoapActionWrongBody(String, u32, String),
    #[error("Soap Error: No envelop for action {0}")]
    SoapNoEnvelop(String),
    #[error("ArilycTcp Error: {0}")]
    ArilycTcpError(String),
    #[error("LinkPlay Error: {0}")]
    LinkPlayError(String),
    #[error("Chromecast Error: {0}")]
    ChromecastError(String),
    #[error("MediaServer Error: {0}")]
    MediaServerError(String),
    #[error("Queue Error: {0}")]
    QueueError(String),
    #[error("Invalid time format: {0}")]
    InvalidTimeFormat(String),
    #[error("Error on snapshot: {0}")]
    SnapshotError(String),
    #[error("Error on ControlPoint: {0}")]
    ControlPoint(String),
}

impl ControlPointError {
    pub fn upnp_operation_not_supported(operation: &str, service: &str) -> Self {
        ControlPointError::UpnpOperationNotSupported(operation.to_string(), service.to_string())
    }

    pub fn upnp_missing_return_value(value: &str) -> Self {
        ControlPointError::UpnpMissingReturnValue(value.to_string())
    }

    pub fn upnp_bad_return_value(name: &str, value: &str) -> Self {
        ControlPointError::UpnpBadReturnValue(name.to_string(), value.to_string())
    }

    pub fn arilyc_tcp_error(message: &str) -> Self {
        ControlPointError::ArilycTcpError(message.to_string())
    }
}
