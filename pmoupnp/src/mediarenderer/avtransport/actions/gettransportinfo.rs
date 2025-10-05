use crate::mediarenderer::avtransport::variables::{A_ARG_TYPE_INSTANCE_ID, TRANSPORTSTATE, TRANSPORTSTATUS};
use crate::define_action;

define_action! {
    pub static GETTRANSPORTINFO = "GetTransportInfo" {
        in "InstanceID" => A_ARG_TYPE_INSTANCE_ID,
        out "CurrentTransportState" => TRANSPORTSTATE,
        out "CurrentTransportStatus" => TRANSPORTSTATUS,
    }
}
