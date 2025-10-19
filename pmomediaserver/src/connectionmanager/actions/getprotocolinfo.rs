use crate::connectionmanager::variables::{SOURCEPROTOCOLINFO, SINKPROTOCOLINFO};
use pmoupnp::define_action;

define_action! {
    pub static GETPROTOCOLINFO = "GetProtocolInfo" stateless {
        out "Source" => SOURCEPROTOCOLINFO,
        out "Sink" => SINKPROTOCOLINFO,
    }
}
