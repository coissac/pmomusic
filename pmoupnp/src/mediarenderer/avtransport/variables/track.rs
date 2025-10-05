use crate::define_variable;

define_variable! {
    pub static CURRENTTRACK: String = "CurrentTrack" {
        evented: true,
    }
}

define_variable! {
    pub static NUMBEROFTRACKS: String = "NumberOfTracks" {
        evented: true,
    }
}

