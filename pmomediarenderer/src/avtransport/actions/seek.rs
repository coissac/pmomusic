use crate::avtransport::variables::{A_ARG_TYPE_INSTANCE_ID, A_ARG_TYPE_SEEKMODE, CURRENTTRACKDURATION};
use pmoupnp::define_action;

define_action! {
    pub static SEEK = "Seek" {
        in "InstanceID" => A_ARG_TYPE_INSTANCE_ID,
        in "Unit" => A_ARG_TYPE_SEEKMODE,
        in "Target" => CURRENTTRACKDURATION,
    }
}
