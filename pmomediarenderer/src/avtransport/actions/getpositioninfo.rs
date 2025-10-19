use crate::avtransport::variables::{
    A_ARG_TYPE_INSTANCE_ID, ABSOLUTETIMEPOSITION, AVTRANSPORTURI, AVTRANSPORTURIMETADATA,
    CURRENTTRACK, CURRENTTRACKDURATION, RELATIVETIMEPOSITION,
};
use pmoupnp::define_action;

define_action! {
    pub static GETPOSITIONINFO = "GetPositionInfo" {
        in "InstanceID" => A_ARG_TYPE_INSTANCE_ID,
        out "Track" => CURRENTTRACK,
        out "TrackDuration" => CURRENTTRACKDURATION,
        out "TrackURI" => AVTRANSPORTURI,
        out "TrackMetaData" => AVTRANSPORTURIMETADATA,
        out "RelTime" => RELATIVETIMEPOSITION,
        out "AbsTime" => ABSOLUTETIMEPOSITION,
    }
}
