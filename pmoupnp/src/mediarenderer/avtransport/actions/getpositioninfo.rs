use crate::mediarenderer::avtransport::variables::{A_ARG_TYPE_INSTANCE_ID, CURRENTTRACK, CURRENTTRACKDURATION, AVTRANSPORTURI, AVTRANSPORTURIMETADATA, RELATIVETIMEPOSITION, ABSOLUTETIMEPOSITION};
use crate::define_action;

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
