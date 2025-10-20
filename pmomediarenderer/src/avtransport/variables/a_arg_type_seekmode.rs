use pmoupnp::define_variable;

define_variable! {
    pub static A_ARG_TYPE_SEEKMODE: String = "A_ARG_TYPE_SeekMode" {
        allowed: ["TRACK_NR", "REL_TIME", "ABS_TIME"],
    }
}
