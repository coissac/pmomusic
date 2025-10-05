use crate::define_variable;

define_variable! {
    pub static A_ARG_TYPE_CONNECTIONSTATUS: String = "A_ARG_TYPE_ConnectionStatus" {
        allowed: ["OK", "ContentFormatMismatch", "InsufficientBandwidth", "UnreliableChannel", "Unknown"],
    }
}
