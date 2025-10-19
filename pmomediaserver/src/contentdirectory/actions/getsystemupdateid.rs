use crate::contentdirectory::variables::SYSTEMUPDATEID;
use pmoupnp::define_action;
use crate::contentdirectory::handlers;

define_action! {
    pub static GETSYSTEMUPDATEID = "GetSystemUpdateID" stateless {
        out "Id" => SYSTEMUPDATEID,
    }
    with handler handlers::get_system_update_id_handler()
}
