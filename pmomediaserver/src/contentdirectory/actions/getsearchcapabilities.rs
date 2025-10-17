use crate::contentdirectory::variables::SEARCHCAPABILITIES;
use pmoupnp::define_action;

define_action! {
    pub static GETSEARCHCAPABILITIES = "GetSearchCapabilities" {
        out "SearchCaps" => SEARCHCAPABILITIES,
    }
}
