use crate::contentdirectory::variables::SEARCHCAPABILITIES;
use pmoupnp::define_action;
use crate::contentdirectory::handlers;

define_action! {
    pub static GETSEARCHCAPABILITIES = "GetSearchCapabilities" stateless {
        out "SearchCaps" => SEARCHCAPABILITIES,
    }
    with handler handlers::get_search_capabilities_handler()
}
