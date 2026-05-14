use std::{fmt, net::IpAddr, str::FromStr};

use allenheath_dlive::channels::{Channel, ChannelType};
use anyhow::Context;
use clap::Parser;

#[derive(Debug, Clone, Parser)]
#[command(author, version, about)]
pub struct Args {
    /// Authentication token for clients
    #[arg(long, env = "AUTH_TOKEN")]
    pub token: Option<String>,

    /// IP address of the mix rack
    #[arg(long, env = "DLIVE_IP")]
    pub ip: IpAddr,

    /// Input channels
    #[arg(long, env = "DLIVE_INPUTS")]
    pub inputs: ChannelRangeList,

    /// Mix channels
    #[arg(long, env = "DLIVE_MIXES")]
    pub mixes: ChannelRangeList,
}

impl Args {
    pub fn channel_config(&self) -> ChannelConfig {
        ChannelConfig {
            inputs: self.inputs.clone(),
            mixes: self.mixes.clone(),
        }
    }
}

/// The subset of configuration that can be reloaded at runtime via SIGHUP.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelConfig {
    pub inputs: ChannelRangeList,
    pub mixes: ChannelRangeList,
}

impl ChannelConfig {
    /// Re-reads `DLIVE_INPUTS` and `DLIVE_MIXES` from the `.env` file without
    /// mutating the process environment.
    pub fn load() -> anyhow::Result<Self> {
        let mut inputs: Option<ChannelRangeList> = None;
        let mut mixes: Option<ChannelRangeList> = None;

        for item in dotenvy::dotenv_iter().context("failed to open .env file")? {
            let (key, value) = item.context("failed to parse .env entry")?;
            match key.as_str() {
                "DLIVE_INPUTS" => inputs = Some(value.parse().context("DLIVE_INPUTS")?),
                "DLIVE_MIXES" => mixes = Some(value.parse().context("DLIVE_MIXES")?),
                _ => {}
            }
        }

        Ok(ChannelConfig {
            inputs: inputs.context("DLIVE_INPUTS not set in .env")?,
            mixes: mixes.context("DLIVE_MIXES not set in .env")?,
        })
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct ChannelRangeList {
    ranges: Vec<ChannelRange>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelRange {
    ty: ChannelType,
    start: u8,
    end: u8,
    step: u8,
}

impl ChannelRangeList {
    pub fn iter(&self) -> impl Iterator<Item = Channel> {
        self.ranges.iter().flat_map(ChannelRange::iter)
    }
}

impl ChannelRange {
    pub fn iter(&self) -> impl Iterator<Item = Channel> {
        (self.start..=self.end)
            .step_by(self.step as usize)
            .map(|n| Channel(self.ty, n))
    }
}

impl fmt::Debug for ChannelRangeList {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&self.ranges, f)
    }
}

impl fmt::Display for ChannelRangeList {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for (i, range) in self.ranges.iter().enumerate() {
            if i != 0 {
                write!(f, ",")?;
            }
            write!(f, "{range}")?;
        }
        Ok(())
    }
}

impl fmt::Display for ChannelRange {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.step == 2 {
            write!(f, "St")?;
        }
        if self.start == self.end {
            write!(f, "{}{}", self.ty, self.start)?;
        } else {
            write!(f, "{}{}-{}", self.ty, self.start, self.end)?;
        }
        Ok(())
    }
}

impl FromStr for ChannelRangeList {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let ranges = s
            .split(',')
            .map(|part| part.parse::<ChannelRange>())
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ChannelRangeList { ranges })
    }
}

impl FromStr for ChannelRange {
    type Err = anyhow::Error;

    fn from_str(mut s: &str) -> Result<Self, Self::Err> {
        let step;
        if s.starts_with("StIp") {
            s = s.strip_prefix("St").unwrap();
            step = 2;
        } else {
            step = 1;
        }

        let prefix_len = s
            .find(|c: char| c.is_digit(10))
            .context("channel name has no number")?;
        let (ty_str, rest) = s.split_at(prefix_len);
        let ty = ty_str.parse()?;

        let (start, end) = if let Some((a, b)) = rest.split_once('-') {
            (a.parse::<u8>()?, b.parse::<u8>()?)
        } else {
            let n = rest.parse::<u8>()?;
            (n, n)
        };

        Channel(ty, start).validate()?;
        Channel(ty, end).validate()?;

        Ok(ChannelRange {
            ty,
            start,
            end,
            step,
        })
    }
}

#[cfg(test)]
mod tests {
    use allenheath_dlive::channels::{Channel, ChannelType};

    use crate::args::{ChannelRange, ChannelRangeList};

    #[test]
    fn test_format_channel_range_list() {
        let ranges = ChannelRangeList {
            ranges: vec![
                ChannelRange {
                    ty: ChannelType::Dca,
                    start: 16,
                    end: 20,
                    step: 1,
                },
                ChannelRange {
                    ty: ChannelType::Input,
                    start: 49,
                    end: 68,
                    step: 1,
                },
                ChannelRange {
                    ty: ChannelType::Input,
                    start: 69,
                    end: 85,
                    step: 2,
                },
                ChannelRange {
                    ty: ChannelType::Input,
                    start: 123,
                    end: 123,
                    step: 1,
                },
            ],
        };
        let s = "DCA16-20,Ip49-68,StIp69-85,Ip123";

        assert_eq!(ranges.to_string(), s);
    }

    #[test]
    fn test_parse_channel_range_list() {
        let ranges = ChannelRangeList {
            ranges: vec![
                ChannelRange {
                    ty: ChannelType::Dca,
                    start: 16,
                    end: 20,
                    step: 1,
                },
                ChannelRange {
                    ty: ChannelType::Input,
                    start: 49,
                    end: 68,
                    step: 1,
                },
                ChannelRange {
                    ty: ChannelType::Input,
                    start: 69,
                    end: 85,
                    step: 2,
                },
                ChannelRange {
                    ty: ChannelType::Input,
                    start: 123,
                    end: 123,
                    step: 1,
                },
            ],
        };
        let s = "DCA16-20,Ip49-68,StIp69-85,Ip123";

        assert_eq!(s.parse::<ChannelRangeList>().unwrap(), ranges);
    }

    #[test]
    fn test_iter_input_range() {
        let range = "Ip3-7".parse::<ChannelRange>().unwrap();
        assert_eq!(
            range.iter().collect::<Vec<_>>(),
            vec![
                Channel(ChannelType::Input, 3),
                Channel(ChannelType::Input, 4),
                Channel(ChannelType::Input, 5),
                Channel(ChannelType::Input, 6),
                Channel(ChannelType::Input, 7),
            ]
        );
    }

    #[test]
    fn test_iter_stereo_input_range() {
        let range = "StIp3-7".parse::<ChannelRange>().unwrap();
        assert_eq!(
            range.iter().collect::<Vec<_>>(),
            vec![
                Channel(ChannelType::Input, 3),
                Channel(ChannelType::Input, 5),
                Channel(ChannelType::Input, 7),
            ]
        );
    }

    #[test]
    fn test_iter_aux_range() {
        let range = "Aux3-7".parse::<ChannelRange>().unwrap();
        assert_eq!(
            range.iter().collect::<Vec<_>>(),
            vec![
                Channel(ChannelType::MonoAux, 3),
                Channel(ChannelType::MonoAux, 4),
                Channel(ChannelType::MonoAux, 5),
                Channel(ChannelType::MonoAux, 6),
                Channel(ChannelType::MonoAux, 7),
            ]
        );
    }

    #[test]
    fn test_iter_stereo_aux_range() {
        let range = "StAux3-7".parse::<ChannelRange>().unwrap();
        assert_eq!(
            range.iter().collect::<Vec<_>>(),
            vec![
                Channel(ChannelType::StereoAux, 3),
                Channel(ChannelType::StereoAux, 4),
                Channel(ChannelType::StereoAux, 5),
                Channel(ChannelType::StereoAux, 6),
                Channel(ChannelType::StereoAux, 7),
            ]
        );
    }
}
