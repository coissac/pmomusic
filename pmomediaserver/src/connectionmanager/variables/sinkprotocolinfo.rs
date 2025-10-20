use pmoupnp::define_variable;

// Pour un MediaServer, SinkProtocolInfo est vide (le server ne consomme pas de contenu)
define_variable! {
    pub static SINKPROTOCOLINFO: String = "SinkProtocolInfo" {
        evented: true,
    }
}
