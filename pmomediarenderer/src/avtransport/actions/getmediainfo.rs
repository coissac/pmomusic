use crate::avtransport::variables::{
    AVTRANSPORTNEXTURI, AVTRANSPORTNEXTURIMETADATA, AVTRANSPORTURI, AVTRANSPORTURIMETADATA,
    A_ARG_TYPE_INSTANCE_ID, CURRENTTRACK, NUMBEROFTRACKS,
};
use pmoupnp::define_action;

define_action! {
    pub static GETMEDIAINFO = "GetMediaInfo" {
        in "InstanceID" => A_ARG_TYPE_INSTANCE_ID,
        out "NrTracks" => NUMBEROFTRACKS,
        out "CurrentTrack" => CURRENTTRACK,
        out "CurrentURI" => AVTRANSPORTURI,
        out "CurrentURIMetaData" => AVTRANSPORTURIMETADATA,
        out "NextURI" => AVTRANSPORTNEXTURI,
        out "NextURIMetaData" => AVTRANSPORTNEXTURIMETADATA,
    }
}
