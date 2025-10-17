use pmoupnp::define_variable;

// Pour un MediaServer, SourceProtocolInfo liste les protocoles qu'il peut servir
define_variable! {
    pub static SOURCEPROTOCOLINFO: String = "SourceProtocolInfo" {
        evented: true,
    }
}
