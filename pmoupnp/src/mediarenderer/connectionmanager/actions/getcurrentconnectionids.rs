use crate::mediarenderer::connectionmanager::variables::CURRENTCONNECTIONIDS;
use crate::define_action;

define_action! {
    pub static GETCURRENTCONNECTIONIDS = "GetCurrentConnectionIDs" {
        out "ConnectionIDs" => CURRENTCONNECTIONIDS,
    }
}
