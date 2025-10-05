use crate::define_variable;

define_variable! {
    pub static A_ARG_TYPE_CHANNEL: String = "A_ARG_TYPE_Channel" {
        allowed: ["Master", "LF", "RF"],
        default: "Master",
    }
}
