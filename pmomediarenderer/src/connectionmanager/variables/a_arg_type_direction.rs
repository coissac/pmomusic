use pmoupnp::define_variable;

define_variable! {
    pub static A_ARG_TYPE_DIRECTION: String = "A_ARG_TYPE_Direction" {
        allowed: ["Input", "Output"],
    }
}
