use crate::connectionmanager::variables::CURRENTCONNECTIONIDS;
use pmoupnp::define_action;

define_action! {
    pub static GETCURRENTCONNECTIONIDS = "GetCurrentConnectionIDs" {
        out "ConnectionIDs" => CURRENTCONNECTIONIDS,
    }
}
