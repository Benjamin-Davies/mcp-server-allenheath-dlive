use std::{fmt, net::IpAddr, str::FromStr};

use allenheath_dlive::channels::{Channel, ChannelType};
use anyhow::Context;
use clap::Parser;

#[derive(Debug, Clone, Parser)]
#[command(author, version, about)]
pub struct Args {
    /// Authentication token for clients
    #[arg(long, env = "AUTH_TOKEN")]
    pub token: String,

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

#[derive(Clone, PartialEq, Eq)]
pub struct ChannelRangeList {
    ranges: Vec<ChannelRange>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelRange(ChannelType, u8, u8);

impl ChannelRangeList {
    pub fn iter(&self) -> impl Iterator<Item = Channel> {
        self.ranges.iter().flat_map(ChannelRange::iter)
    }
}

impl ChannelRange {
    pub fn iter(&self) -> impl Iterator<Item = Channel> {
        let step = match self.0 {
            ChannelType::StereoInput => 2,
            _ => 1,
        };
        (self.1..=self.2).step_by(step).map(|n| Channel(self.0, n))
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
        if self.1 == self.2 {
            write!(f, "{}{}", self.0, self.1)?;
        } else {
            write!(f, "{}{}-{}", self.0, self.1, self.2)?;
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

    fn from_str(s: &str) -> Result<Self, Self::Err> {
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

        Ok(ChannelRange(ty, start, end))
    }
}

#[cfg(test)]
mod tests {
    use allenheath_dlive::channels::ChannelType;

    use crate::args::{ChannelRange, ChannelRangeList};

    #[test]
    fn test_format_channel_range_list() {
        let ranges = ChannelRangeList {
            ranges: vec![
                ChannelRange(ChannelType::Dca, 16, 20),
                ChannelRange(ChannelType::Input, 49, 68),
                ChannelRange(ChannelType::StereoInput, 69, 85),
                ChannelRange(ChannelType::Input, 123, 123),
            ],
        };
        let s = "Dca16-20,Ip49-68,StIp69-85,Ip123";

        assert_eq!(ranges.to_string(), s);
    }

    #[test]
    fn test_parse_channel_range_list() {
        let ranges = ChannelRangeList {
            ranges: vec![
                ChannelRange(ChannelType::Dca, 16, 20),
                ChannelRange(ChannelType::Input, 49, 68),
                ChannelRange(ChannelType::StereoInput, 69, 85),
                ChannelRange(ChannelType::Input, 123, 123),
            ],
        };
        let s = "Dca16-20,Ip49-68,StIp69-85,Ip123";

        assert_eq!(s.parse::<ChannelRangeList>().unwrap(), ranges);
    }
}
