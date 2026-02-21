use crate::avtransport::variables::{
    A_ARG_TYPE_INSTANCE_ID, TRANSPORTPLAYSPEED, TRANSPORTSTATE, TRANSPORTSTATUS,
};
use pmoupnp::define_action;

define_action! {
    pub static GETTRANSPORTINFO = "GetTransportInfo" {
        in "InstanceID" => A_ARG_TYPE_INSTANCE_ID,
        out "CurrentTransportState" => TRANSPORTSTATE,
        out "CurrentTransportStatus" => TRANSPORTSTATUS,
        out "CurrentSpeed" => TRANSPORTPLAYSPEED,
    }
}
