use crate::renderingcontrol::variables::{A_ARG_TYPE_INSTANCE_ID, A_ARG_TYPE_CHANNEL, MUTE};
use pmoupnp::define_action;

define_action! {
    pub static SETMUTE = "SetMute" {
        in "InstanceID" => A_ARG_TYPE_INSTANCE_ID,
        in "Channel" => A_ARG_TYPE_CHANNEL,
        in "DesiredMute" => MUTE,
    }
}
