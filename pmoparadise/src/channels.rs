//! Radio Paradise channel definitions
//!
//! This module defines the available Radio Paradise channels and their metadata.

use std::str::FromStr;

/// Logical identifier for a Radio Paradise channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParadiseChannelKind {
    Main,
    Mellow,
    Rock,
    Eclectic,
}

impl ParadiseChannelKind {
    pub const fn id(self) -> u8 {
        match self {
            Self::Main => 0,
            Self::Mellow => 1,
            Self::Rock => 2,
            Self::Eclectic => 3,
        }
    }

    pub const fn slug(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Mellow => "mellow",
            Self::Rock => "rock",
            Self::Eclectic => "eclectic",
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Main => "Main Mix",
            Self::Mellow => "Mellow Mix",
            Self::Rock => "Rock Mix",
            Self::Eclectic => "Eclectic Mix",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::Main => "Eclectic mix of rock, world, electronica, and more",
            Self::Mellow => "Mellower, less aggressive music",
            Self::Rock => "Heavier, more guitar-driven music",
            Self::Eclectic => "Curated worldwide selection",
        }
    }
}

impl FromStr for ParadiseChannelKind {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "main" | "0" => Ok(Self::Main),
            "mellow" | "1" => Ok(Self::Mellow),
            "rock" | "2" => Ok(Self::Rock),
            "eclectic" | "3" => Ok(Self::Eclectic),
            other => Err(anyhow::anyhow!("Unknown Radio Paradise channel: {}", other)),
        }
    }
}

/// Metadata descriptor for a channel.
#[derive(Debug, Clone, Copy)]
pub struct ChannelDescriptor {
    pub kind: ParadiseChannelKind,
    pub id: u8,
    pub slug: &'static str,
    pub display_name: &'static str,
    pub description: &'static str,
}

impl ChannelDescriptor {
    pub const fn new(kind: ParadiseChannelKind) -> Self {
        Self {
            id: kind.id(),
            slug: kind.slug(),
            display_name: kind.display_name(),
            description: kind.description(),
            kind,
        }
    }
}

/// All available Radio Paradise channels
pub const ALL_CHANNELS: [ChannelDescriptor; 4] = [
    ChannelDescriptor::new(ParadiseChannelKind::Main),
    ChannelDescriptor::new(ParadiseChannelKind::Mellow),
    ChannelDescriptor::new(ParadiseChannelKind::Rock),
    ChannelDescriptor::new(ParadiseChannelKind::Eclectic),
];

/// Returns the maximum valid channel ID
pub const fn max_channel_id() -> u8 {
    (ALL_CHANNELS.len() - 1) as u8
}

/// Default maximum number of tracks to keep in history
///
/// This is used as the default if not configured via pmoconfig.
/// Value: 100 tracks - represents ~5-8 hours of playback history
pub const HISTORY_DEFAULT_MAX_TRACKS: usize = 100;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_ids() {
        assert_eq!(ParadiseChannelKind::Main.id(), 0);
        assert_eq!(ParadiseChannelKind::Mellow.id(), 1);
        assert_eq!(ParadiseChannelKind::Rock.id(), 2);
        assert_eq!(ParadiseChannelKind::Eclectic.id(), 3);
    }

    #[test]
    fn test_max_channel_id() {
        assert_eq!(max_channel_id(), 3);
    }

    #[test]
    fn test_all_channels_length() {
        assert_eq!(ALL_CHANNELS.len(), 4);
    }

    #[test]
    fn test_channel_from_str() {
        assert!(matches!(
            "main".parse::<ParadiseChannelKind>(),
            Ok(ParadiseChannelKind::Main)
        ));
        assert!(matches!(
            "0".parse::<ParadiseChannelKind>(),
            Ok(ParadiseChannelKind::Main)
        ));
        assert!("invalid".parse::<ParadiseChannelKind>().is_err());
    }
}
