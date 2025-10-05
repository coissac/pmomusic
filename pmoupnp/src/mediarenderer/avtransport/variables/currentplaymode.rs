use crate::define_variable;

define_variable! {
    pub static CURRENTPLAYMODE: String = "CurrentPlayMode" {
        allowed: ["NORMAL", "SHUFFLE", "REPEAT_ONE", "REPEAT_ALL", "RANDOM", "DIRECT_1", "INTRO"],
        default: "NORMAL",
    }
}
