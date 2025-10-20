use crate::avtransport::variables::{A_ARG_TYPE_INSTANCE_ID, TRANSPORTPLAYSPEED};
use pmoupnp::define_action;

define_action! {
    pub static PLAY = "Play" {
        in "InstanceID" => A_ARG_TYPE_INSTANCE_ID,
        in "Speed" => TRANSPORTPLAYSPEED,
    }
}
