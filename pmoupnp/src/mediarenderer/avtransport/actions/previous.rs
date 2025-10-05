use crate::mediarenderer::avtransport::variables::A_ARG_TYPE_INSTANCE_ID;
use crate::define_action;

define_action! {
    pub static PREVIOUS = "Previous"  {
        in "InstanceID" => A_ARG_TYPE_INSTANCE_ID,
    }
}
