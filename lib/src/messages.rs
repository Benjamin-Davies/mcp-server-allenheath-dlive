const BASE_MIDI_CHANNEL: u8 = 11;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Channel {
    Input(u8),
    MonoGroup(u8),
    StereoGroup(u8),
    MonoAux(u8),
    StereoAux(u8),
    MonoMatrix(u8),
    StereoMatrix(u8),
    MonoFxSend(u8),
    StereoFxSend(u8),
    FxReturn(u8),
    Mains(u8),
    Dca(u8),
    MuteGroup(u8),
    StereoUFXSend(u8),
    StereoUFXReturn(u8),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Level(pub u8);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    GetChannelName {
        channel: Channel,
    },
    ChannelName {
        channel: Channel,
        name: String,
    },
    GetSendLevel {
        channel: Channel,
        send: Channel,
    },
    SendLevel {
        channel: Channel,
        send: Channel,
        level: Level,
    },
}

macro_rules! channel_mappings {
    (
        $(
            $kind:ident ( $start:literal ..= $end:literal ) => (
                $n_offset:literal ,
                $ch_start:literal ..= $ch_end:literal
            ),
        )*
    ) => {
        impl Channel {
            pub fn validate(self) -> anyhow::Result<()> {
                match self {
                    $(
                        Channel::$kind(n) => anyhow::ensure!(
                            ($start..=$end).contains(&n),
                            "{} channels must be between {}..={}",
                            stringify!($kind),
                            $start,
                            $end
                        ),
                    )*
                }
                Ok(())
            }

            pub(super) fn to_midi(self) -> anyhow::Result<(u8, u8)> {
                self.validate()?;
                match self {
                    $(
                        Self::$kind(n) => Ok((BASE_MIDI_CHANNEL + $n_offset, n - 1 + $ch_start)),
                    )*
                }
            }

            pub(super) fn from_midi(midi_channel: u8, note: u8) -> anyhow::Result<Self> {
                match (midi_channel.wrapping_sub(BASE_MIDI_CHANNEL) & 0x0F, note) {
                    $(
                        ($n_offset, $ch_start..=$ch_end) => Ok(Self::$kind(note - $ch_start + 1)),
                    )*
                    _ => anyhow::bail!("Unknown channel. MIDI channel: {midi_channel}, note: {note}"),
                }
            }
        }
    };
}

channel_mappings! {
    Input           (1..=128) => (0, 0x00..=0x7F),
    MonoGroup       (1..=62)  => (1, 0x00..=0x3D),
    StereoGroup     (1..=31)  => (1, 0x40..=0x5E),
    MonoAux         (1..=62)  => (2, 0x00..=0x3D),
    StereoAux       (1..=31)  => (2, 0x40..=0x5E),
    MonoMatrix      (1..=62)  => (3, 0x00..=0x3D),
    StereoMatrix    (1..=31)  => (3, 0x40..=0x5E),
    MonoFxSend      (1..=16)  => (4, 0x00..=0x0F),
    StereoFxSend    (1..=16)  => (4, 0x10..=0x1F),
    FxReturn        (1..=16)  => (4, 0x20..=0x2F),
    Mains           (1..=6)   => (4, 0x30..=0x35),
    Dca             (1..=24)  => (4, 0x36..=0x4D),
    MuteGroup       (1..=8)   => (4, 0x4E..=0x55),
    StereoUFXSend   (1..=8)   => (4, 0x56..=0x5D),
    StereoUFXReturn (1..=8)   => (4, 0x5E..=0x65),
}

/// Converts a dLive 7-bit fader value (0x00–0x7F) to dB.
/// 0x00 = -inf, 0x6B = 0dB, 0x7F = +10dB.
fn dlive_value_to_db(value: u8) -> f32 {
    if value == 0 {
        return f32::NEG_INFINITY;
    }
    (value as f32 - 107.0) / 2.0
}

/// Converts a dB value to a dLive 7-bit fader value.
fn db_to_dlive_value(gain: f32) -> u8 {
    if gain == f32::NEG_INFINITY || gain < -90.0 {
        return 0x00;
    }
    (2.0 * gain + 107.0).clamp(0.0, 127.0).round() as u8
}

impl From<Level> for f32 {
    fn from(value: Level) -> Self {
        dlive_value_to_db(value.0)
    }
}

impl From<f32> for Level {
    fn from(value: f32) -> Self {
        Self(db_to_dlive_value(value))
    }
}

#[cfg(test)]
mod tests {
    use crate::messages::{Channel, db_to_dlive_value, dlive_value_to_db};

    #[test]
    fn test_channel_is_valid() {
        assert_eq!(Channel::Input(0).validate().is_ok(), false);
        assert_eq!(Channel::Input(1).validate().is_ok(), true);
        assert_eq!(Channel::Input(128).validate().is_ok(), true);
        assert_eq!(Channel::Input(129).validate().is_ok(), false);
        assert_eq!(Channel::StereoAux(0).validate().is_ok(), false);
        assert_eq!(Channel::StereoAux(1).validate().is_ok(), true);
        assert_eq!(Channel::StereoAux(31).validate().is_ok(), true);
        assert_eq!(Channel::StereoAux(32).validate().is_ok(), false);
    }

    #[test]
    fn test_channel_to_midi() {
        assert!(Channel::Input(0).to_midi().is_err());
        assert_eq!(Channel::Input(1).to_midi().unwrap(), (11, 0x00));
        assert_eq!(Channel::Input(128).to_midi().unwrap(), (11, 0x7F));
        assert!(Channel::Input(129).to_midi().is_err());
        assert!(Channel::StereoAux(0).to_midi().is_err());
        assert_eq!(Channel::StereoAux(1).to_midi().unwrap(), (13, 0x40));
        assert_eq!(Channel::StereoAux(31).to_midi().unwrap(), (13, 0x5E));
        assert!(Channel::StereoAux(32).to_midi().is_err());
    }

    #[test]
    fn test_channel_from_midi() {
        assert_eq!(Channel::from_midi(11, 0x00).unwrap(), Channel::Input(1));
        assert_eq!(Channel::from_midi(11, 0x7F).unwrap(), Channel::Input(128));
        assert!(Channel::from_midi(11, 0x80).is_err());
        assert_eq!(Channel::from_midi(13, 0x40).unwrap(), Channel::StereoAux(1));
        assert_eq!(
            Channel::from_midi(13, 0x5E).unwrap(),
            Channel::StereoAux(31)
        );
        assert!(Channel::from_midi(13, 0x5F).is_err());
    }

    #[test]
    fn test_dlive_value_to_db() {
        assert_eq!(dlive_value_to_db(0x7F), 10.0);
        assert_eq!(dlive_value_to_db(0x75), 5.0);
        assert_eq!(dlive_value_to_db(0x6B), 0.0);
        assert_eq!(dlive_value_to_db(0x61), -5.0);
        assert_eq!(dlive_value_to_db(0x57), -10.0);
        assert_eq!(dlive_value_to_db(0x4D), -15.0);
        assert_eq!(dlive_value_to_db(0x43), -20.0);
        assert_eq!(dlive_value_to_db(0x39), -25.0);
        assert_eq!(dlive_value_to_db(0x2F), -30.0);
        assert_eq!(dlive_value_to_db(0x25), -35.0);
        assert_eq!(dlive_value_to_db(0x1B), -40.0);
        assert_eq!(dlive_value_to_db(0x11), -45.0);
        assert_eq!(dlive_value_to_db(0x00), f32::NEG_INFINITY);
    }

    #[test]
    fn test_db_to_dlive_value() {
        assert_eq!(db_to_dlive_value(10.0), 0x7F);
        assert_eq!(db_to_dlive_value(5.0), 0x75);
        assert_eq!(db_to_dlive_value(0.0), 0x6B);
        assert_eq!(db_to_dlive_value(-5.0), 0x61);
        assert_eq!(db_to_dlive_value(-10.0), 0x57);
        assert_eq!(db_to_dlive_value(-15.0), 0x4D);
        assert_eq!(db_to_dlive_value(-20.0), 0x43);
        assert_eq!(db_to_dlive_value(-25.0), 0x39);
        assert_eq!(db_to_dlive_value(-30.0), 0x2F);
        assert_eq!(db_to_dlive_value(-35.0), 0x25);
        assert_eq!(db_to_dlive_value(-40.0), 0x1B);
        assert_eq!(db_to_dlive_value(-45.0), 0x11);
        assert_eq!(db_to_dlive_value(f32::NEG_INFINITY), 0x00);
    }
}
