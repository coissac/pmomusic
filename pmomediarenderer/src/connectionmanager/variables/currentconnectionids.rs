use pmoupnp::define_variable;

define_variable! {
    pub static CURRENTCONNECTIONIDS: String = "CurrentConnectionIDs" {
        evented: true,
    }
}
