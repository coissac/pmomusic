use crate::mediarenderer::connectionmanager::variables::{SOURCEPROTOCOLINFO, SINKPROTOCOLINFO};
use crate::define_action;

define_action! {
    pub static GETPROTOCOLINFO = "GetProtocolInfo" {
        out "Source" => SOURCEPROTOCOLINFO,
        out "Sink" => SINKPROTOCOLINFO,
    }
}
