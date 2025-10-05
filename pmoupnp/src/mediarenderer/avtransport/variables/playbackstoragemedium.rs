use crate::define_variable;

// Valeurs pour un MediaRenderer audio uniquement (suppression des formats vidéo)
define_variable! {
    pub static PLAYBACKSTORAGEMEDIUM: String = "PlaybackStorageMedium" {
        allowed: [
            "UNKNOWN", "CD-ROM", "CD-DA", "CD-R", "CD-RW", "SACD",
            "MD-AUDIO", "DVD-AUDIO", "DAT", "HDD", "NETWORK",
            "NONE", "NOT_IMPLEMENTED"
        ],
    }
}
