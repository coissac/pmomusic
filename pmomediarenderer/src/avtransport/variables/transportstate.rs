use pmoupnp::define_variable;

// États pour un MediaRenderer audio uniquement (suppression des états d'enregistrement)
define_variable! {
    pub static TRANSPORTSTATE: String = "TransportState" {
        allowed: ["STOPPED", "PLAYING", "TRANSITIONING", "PAUSED_PLAYBACK", "NO_MEDIA_PRESENT"],
        evented: true,
    }
}

