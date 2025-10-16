use pmoupnp::define_variable;

define_variable! {
    pub static TRANSPORTPLAYSPEED: String = "TransportPlaySpeed" {
        allowed: ["1"],
        default: "1",
    }
}

