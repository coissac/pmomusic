use pmoupnp::define_variable;

define_variable! {
    pub static CURRENTTRACKDURATION: String = "CurrentTrackDuration" {
        evented: true,
    }
}

define_variable! {
    pub static ABSOLUTETIMEPOSITION: String = "AbsoluteTimePosition" {
        evented: true,
    }
}

define_variable! {
    pub static RELATIVETIMEPOSITION: String = "RelativeTimePosition" {
        evented: true,
    }
}

