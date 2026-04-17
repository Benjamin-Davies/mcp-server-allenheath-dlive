use std::{borrow::Cow, fmt};

use crate::channels::{Channel, ChannelName};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Level(pub u8);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    GetChannelName {
        channel: Channel,
    },
    ChannelName {
        channel: Channel,
        name: ChannelName,
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
    GetFaderLevel {
        channel: Channel,
    },
    FaderLevel {
        channel: Channel,
        level: Level,
    },
}

impl Level {
    pub const ZERO: Self = Self(0x6B);
    pub const NEG_INFINITY: Self = Self(0);
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

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} dB", f32::from(*self))?;
        Ok(())
    }
}

impl serde::Serialize for Level {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if *self == Self::NEG_INFINITY {
            "-inf".serialize(serializer)
        } else {
            f32::from(*self).serialize(serializer)
        }
    }
}

impl<'de> serde::Deserialize<'de> for Level {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        #[serde(untagged)]
        enum Inner {
            String(String),
            Float(f32),
        }

        match Inner::deserialize(deserializer)? {
            Inner::String(s) if s == "-inf" => Ok(Self::NEG_INFINITY),
            Inner::Float(db @ -50.0..=10.0) => Ok(db.into()),
            _ => Err(serde::de::Error::custom(
                "expected a float between -50 and +10 or the string \"-inf\"",
            )),
        }
    }
}

impl schemars::JsonSchema for Level {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> Cow<'static, str> {
        "Level".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "oneOf": [
                {
                    "$comment": "A number of dB to 1 decimal place",
                    "type": "number",
                    "minimum": -50,
                    "maximum": 10,
                },
                {
                    "$comment": "Muted",
                    "type": "string",
                    "const": "-inf",
                },
            ]
        })
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
