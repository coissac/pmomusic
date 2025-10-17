use crate::contentdirectory::variables::SYSTEMUPDATEID;
use pmoupnp::define_action;

define_action! {
    pub static GETSYSTEMUPDATEID = "GetSystemUpdateID" {
        out "Id" => SYSTEMUPDATEID,
    }
}
