use crate::mediarenderer::avtransport::variables::{A_ARG_TYPE_INSTANCE_ID, TRANSPORTPLAYSPEED};
use crate::define_action;
 
define_action! {
    pub static PLAY = "Play" {
        in "InstanceID" => A_ARG_TYPE_INSTANCE_ID,
        in "Speed" => TRANSPORTPLAYSPEED,
    }
}
