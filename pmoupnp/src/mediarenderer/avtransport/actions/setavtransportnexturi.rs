use crate::mediarenderer::avtransport::variables::{A_ARG_TYPE_INSTANCE_ID, AVTRANSPORTNEXTURI, AVTRANSPORTNEXTURIMETADATA};
use crate::define_action;

define_action! {
    pub static SETNEXTAVTRANSPORTURI = "SetNextAVTransportURI" {
        in "InstanceID" => A_ARG_TYPE_INSTANCE_ID,
        in "NextURI" => AVTRANSPORTNEXTURI,
        in "NextURIMetaData" => AVTRANSPORTNEXTURIMETADATA,
    }
}
