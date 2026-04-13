use std::{fmt, ops::RangeInclusive, str::FromStr};

use anyhow::Context;

const BASE_MIDI_CHANNEL: u8 = 11;

const PREFIXES: &[(ChannelType, &str)] = &[
    (ChannelType::Input, "Ip"),
    (ChannelType::StereoInput, "StIp"),
    (ChannelType::MonoGroup, "Grp"),
    (ChannelType::StereoGroup, "StGrp"),
    (ChannelType::MonoAux, "Aux"),
    (ChannelType::StereoAux, "StAux"),
    (ChannelType::MonoMatrix, "Mtx"),
    (ChannelType::StereoMatrix, "StMtx"),
    (ChannelType::FxReturn, "FXRet"),
    (ChannelType::Mains, "Main"),
    (ChannelType::Dca, "DCA"),
    (ChannelType::StereoUFXSend, "UFX"),
    (ChannelType::StereoUFXReturn, "UFXR"),
];

const MIDI_MAPPINGS: &[(ChannelType, u8, RangeInclusive<u8>)] = &[
    (ChannelType::Input, BASE_MIDI_CHANNEL, 0x00..=0x7F),
    (ChannelType::MonoGroup, BASE_MIDI_CHANNEL + 1, 0x00..=0x3D),
    (ChannelType::StereoGroup, BASE_MIDI_CHANNEL + 1, 0x40..=0x5E),
    (ChannelType::MonoAux, BASE_MIDI_CHANNEL + 2, 0x00..=0x3D),
    (ChannelType::StereoAux, BASE_MIDI_CHANNEL + 2, 0x40..=0x5E),
    (ChannelType::MonoMatrix, BASE_MIDI_CHANNEL + 3, 0x00..=0x3D),
    (
        ChannelType::StereoMatrix,
        BASE_MIDI_CHANNEL + 3,
        0x40..=0x5E,
    ),
    (ChannelType::MonoFxSend, BASE_MIDI_CHANNEL + 4, 0x00..=0x0F),
    (
        ChannelType::StereoFxSend,
        BASE_MIDI_CHANNEL + 4,
        0x10..=0x1F,
    ),
    (ChannelType::FxReturn, BASE_MIDI_CHANNEL + 4, 0x20..=0x2F),
    (ChannelType::Mains, BASE_MIDI_CHANNEL + 4, 0x30..=0x35),
    (ChannelType::Dca, BASE_MIDI_CHANNEL + 4, 0x36..=0x4D),
    (ChannelType::MuteGroup, BASE_MIDI_CHANNEL + 4, 0x4E..=0x55),
    (
        ChannelType::StereoUFXSend,
        BASE_MIDI_CHANNEL + 4,
        0x56..=0x5D,
    ),
    (
        ChannelType::StereoUFXReturn,
        BASE_MIDI_CHANNEL + 4,
        0x5E..=0x65,
    ),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Channel(pub ChannelType, pub u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ChannelType {
    Input,
    /// Internally just a pair of inputs that have been tied together.
    /// Has a spacing of two when iterated.
    StereoInput,
    MonoGroup,
    StereoGroup,
    MonoAux,
    StereoAux,
    MonoMatrix,
    StereoMatrix,
    MonoFxSend,
    StereoFxSend,
    FxReturn,
    Mains,
    Dca,
    MuteGroup,
    StereoUFXSend,
    StereoUFXReturn,
}

impl Channel {
    pub fn validate(self) -> anyhow::Result<()> {
        let range = match self.0 {
            ChannelType::Input | ChannelType::StereoInput => 1..=128,
            ChannelType::MonoGroup | ChannelType::MonoAux | ChannelType::MonoMatrix => 1..=62,
            ChannelType::StereoGroup | ChannelType::StereoAux | ChannelType::StereoMatrix => 1..=31,
            ChannelType::MonoFxSend | ChannelType::StereoFxSend | ChannelType::FxReturn => 1..=16,
            ChannelType::Mains => 1..=6,
            ChannelType::Dca => 1..=24,
            ChannelType::MuteGroup | ChannelType::StereoUFXSend | ChannelType::StereoUFXReturn => {
                1..=8
            }
        };
        anyhow::ensure!(
            range.contains(&self.1),
            "{} channels must be between {range:?}",
            self.0,
        );
        if self.0 == ChannelType::StereoInput {
            anyhow::ensure!(
                self.1 % 2 == 1,
                "{} channels must have an odd number",
                self.0
            );
        }
        Ok(())
    }

    pub(crate) fn to_midi(self) -> anyhow::Result<(u8, u8)> {
        self.validate()?;
        let (_, n, ch_range) = MIDI_MAPPINGS
            .iter()
            .find(|&&(ty, _, _)| ty == self.0)
            .context("unknown channel type")?;
        Ok((*n, self.1 - 1 + ch_range.start()))
    }

    pub(crate) fn from_midi(midi_channel: u8, note: u8) -> anyhow::Result<Self> {
        let (ty, _, ch_range) = MIDI_MAPPINGS
            .iter()
            .find(|&(_, n, ch_range)| n == &midi_channel && ch_range.contains(&note))
            .context("unknown channel type")?;
        Ok(Self(*ty, note - ch_range.start() + 1))
    }
}

impl fmt::Display for Channel {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{}", self.0, self.1)?;
        Ok(())
    }
}

impl fmt::Display for ChannelType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some((_, s)) = PREFIXES.iter().find(|&(ty, _)| ty == self) {
            write!(f, "{s}")?;
        } else {
            write!(f, "{self:?}")?;
        }
        Ok(())
    }
}

impl FromStr for Channel {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let prefix_len = s
            .find(|c: char| c.is_digit(10))
            .context("channel name has no number")?;
        let (ty_str, rest) = s.split_at(prefix_len);
        let ty = ty_str.parse()?;
        let n = rest.parse()?;
        Ok(Channel(ty, n))
    }
}

impl FromStr for ChannelType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let &(ty, _) = PREFIXES
            .iter()
            .find(|&&(_, a)| a == s)
            .with_context(|| format!("unknown channel type {s:?}"))?;
        Ok(ty)
    }
}

#[cfg(test)]
mod tests {
    use crate::channels::{Channel, ChannelType};

    #[test]
    fn test_channel_is_valid() {
        assert_eq!(Channel(ChannelType::Input, 0).validate().is_ok(), false);
        assert_eq!(Channel(ChannelType::Input, 1).validate().is_ok(), true);
        assert_eq!(Channel(ChannelType::Input, 128).validate().is_ok(), true);
        assert_eq!(Channel(ChannelType::Input, 129).validate().is_ok(), false);
        assert_eq!(Channel(ChannelType::StereoAux, 0).validate().is_ok(), false);
        assert_eq!(Channel(ChannelType::StereoAux, 1).validate().is_ok(), true);
        assert_eq!(Channel(ChannelType::StereoAux, 31).validate().is_ok(), true);
        assert_eq!(
            Channel(ChannelType::StereoAux, 32).validate().is_ok(),
            false
        );
    }

    #[test]
    fn test_channel_to_midi() {
        assert!(Channel(ChannelType::Input, 0).to_midi().is_err());
        assert_eq!(
            Channel(ChannelType::Input, 1).to_midi().unwrap(),
            (11, 0x00)
        );
        assert_eq!(
            Channel(ChannelType::Input, 128).to_midi().unwrap(),
            (11, 0x7F)
        );
        assert!(Channel(ChannelType::Input, 129).to_midi().is_err());
        assert!(Channel(ChannelType::StereoAux, 0).to_midi().is_err());
        assert_eq!(
            Channel(ChannelType::StereoAux, 1).to_midi().unwrap(),
            (13, 0x40)
        );
        assert_eq!(
            Channel(ChannelType::StereoAux, 31).to_midi().unwrap(),
            (13, 0x5E)
        );
        assert!(Channel(ChannelType::StereoAux, 32).to_midi().is_err());
    }

    #[test]
    fn test_channel_from_midi() {
        assert_eq!(
            Channel::from_midi(11, 0x00).unwrap(),
            Channel(ChannelType::Input, 1)
        );
        assert_eq!(
            Channel::from_midi(11, 0x7F).unwrap(),
            Channel(ChannelType::Input, 128)
        );
        assert!(Channel::from_midi(11, 0x80).is_err());
        assert_eq!(
            Channel::from_midi(13, 0x40).unwrap(),
            Channel(ChannelType::StereoAux, 1)
        );
        assert_eq!(
            Channel::from_midi(13, 0x5E).unwrap(),
            Channel(ChannelType::StereoAux, 31)
        );
        assert!(Channel::from_midi(13, 0x5F).is_err());
    }
}
