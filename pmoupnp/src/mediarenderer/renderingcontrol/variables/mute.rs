use crate::define_variable;

define_variable! {
    pub static MUTE: Boolean = "Mute" {
        evented: true,
    }
}
