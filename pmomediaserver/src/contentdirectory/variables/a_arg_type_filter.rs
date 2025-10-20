use pmoupnp::define_variable;

define_variable! {
    pub static A_ARG_TYPE_FILTER: String = "A_ARG_TYPE_Filter" {
        default: "*",
        evented: false,
    }
}
