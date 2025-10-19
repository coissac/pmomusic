use pmoupnp::define_variable;

define_variable! {
    pub static A_ARG_TYPE_BROWSEFLAG: String = "A_ARG_TYPE_BrowseFlag" {
        default: "BrowseDirectChildren",
        allowed: ["BrowseMetadata", "BrowseDirectChildren"],
        evented: false,
    }
}
