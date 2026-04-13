use crate::channels::Channel;

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
    use crate::messages::{db_to_dlive_value, dlive_value_to_db};

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
