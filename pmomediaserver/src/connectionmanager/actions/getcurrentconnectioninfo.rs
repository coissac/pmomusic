use crate::connectionmanager::variables::{
    A_ARG_TYPE_CONNECTIONID, A_ARG_TYPE_RCSID, A_ARG_TYPE_AVTRANSPORTID,
    A_ARG_TYPE_PROTOCOLINFO, A_ARG_TYPE_DIRECTION, A_ARG_TYPE_CONNECTIONSTATUS,
};
use pmoupnp::define_action;

define_action! {
    pub static GETCURRENTCONNECTIONINFO = "GetCurrentConnectionInfo" {
        in "ConnectionID" => A_ARG_TYPE_CONNECTIONID,
        out "RcsID" => A_ARG_TYPE_RCSID,
        out "AVTransportID" => A_ARG_TYPE_AVTRANSPORTID,
        out "ProtocolInfo" => A_ARG_TYPE_PROTOCOLINFO,
        out "PeerConnectionManager" => A_ARG_TYPE_CONNECTIONID,
        out "PeerConnectionID" => A_ARG_TYPE_CONNECTIONID,
        out "Direction" => A_ARG_TYPE_DIRECTION,
        out "Status" => A_ARG_TYPE_CONNECTIONSTATUS,
    }
}
