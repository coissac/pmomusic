use crate::mediarenderer::renderingcontrol::variables::{A_ARG_TYPE_INSTANCE_ID, A_ARG_TYPE_CHANNEL, VOLUME};
use crate::define_action;

define_action! {
    pub static SETVOLUME = "SetVolume" {
        in "InstanceID" => A_ARG_TYPE_INSTANCE_ID,
        in "Channel" => A_ARG_TYPE_CHANNEL,
        in "DesiredVolume" => VOLUME,
    }
}
