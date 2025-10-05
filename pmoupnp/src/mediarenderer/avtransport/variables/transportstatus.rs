use crate::define_variable;

define_variable! {
    pub static TRANSPORTSTATUS: String = "TransportStatus" {
        allowed: ["OK", "ERROR_OCCURRED"],
        evented: true,
    }
}
