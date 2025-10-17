use crate::contentdirectory::variables::SORTCAPABILITIES;
use pmoupnp::define_action;

define_action! {
    pub static GETSORTCAPABILITIES = "GetSortCapabilities" {
        out "SortCaps" => SORTCAPABILITIES,
    }
}
