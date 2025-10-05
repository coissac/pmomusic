use crate::define_variable;

// Pour un MediaRenderer audio, liste les protocoles/formats audio supportés
define_variable! {
    pub static SINKPROTOCOLINFO: String = "SinkProtocolInfo" {
        evented: true,
    }
}
