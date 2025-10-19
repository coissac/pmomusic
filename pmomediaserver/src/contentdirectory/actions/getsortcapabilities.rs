use crate::contentdirectory::variables::SORTCAPABILITIES;
use pmoupnp::define_action;
use crate::contentdirectory::handlers;

define_action! {
    pub static GETSORTCAPABILITIES = "GetSortCapabilities" stateless {
        out "SortCaps" => SORTCAPABILITIES,
    }
    with handler handlers::get_sort_capabilities_handler()
}
