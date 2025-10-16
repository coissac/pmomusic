use crate::avtransport::variables::A_ARG_TYPE_INSTANCE_ID;
use pmoupnp::define_action;

define_action! {
    pub static GETCURRENTTRANSPORTACTIONS = "GetCurrentTransportActions" {
        in "InstanceID" => A_ARG_TYPE_INSTANCE_ID,
    }
}
