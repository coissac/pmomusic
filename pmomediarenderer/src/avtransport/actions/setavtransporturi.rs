use crate::avtransport::variables::{A_ARG_TYPE_INSTANCE_ID, AVTRANSPORTURI, AVTRANSPORTURIMETADATA};
use pmoupnp::define_action;

define_action! {
    pub static SETAVTRANSPORTURI = "SetAVTransportURI" {
        in "InstanceID" => A_ARG_TYPE_INSTANCE_ID,
        in "CurrentURI" => AVTRANSPORTURI,
        in "CurrentURIMetaData" => AVTRANSPORTURIMETADATA,
    }
}
