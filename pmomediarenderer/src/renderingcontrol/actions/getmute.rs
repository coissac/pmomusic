use crate::renderingcontrol::variables::{A_ARG_TYPE_CHANNEL, A_ARG_TYPE_INSTANCE_ID, MUTE};
use pmoupnp::define_action;

define_action! {
    pub static GETMUTE = "GetMute" {
        in "InstanceID" => A_ARG_TYPE_INSTANCE_ID,
        in "Channel" => A_ARG_TYPE_CHANNEL,
        out "CurrentMute" => MUTE,
    }
}
