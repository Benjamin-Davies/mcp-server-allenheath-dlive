//! https://www.allen-heath.com/content/uploads/2024/06/dLive-MIDI-Over-TCP-Protocol-V2.0.pdf

use std::{fmt::Debug, io, net::SocketAddr, pin::Pin, time::Duration};

use anyhow::Result;
use futures::{SinkExt, StreamExt};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpStream,
    time::timeout,
};
use tokio_util::codec::Framed;

use crate::{
    channels::{Channel, ChannelName},
    codecs::DLiveCodec,
    messages::{Level, Message},
};

pub mod channels;
pub mod codecs;
pub mod messages;

pub const DLIVE_MIXRACK_TCP_PORT: u16 = 51325;
pub const DLIVE_SURFACE_TCP_PORT: u16 = 51328;
/// Non-standard port. Used specifically for the `fake-dlive` crate.
pub const DLIVE_FAKE_TCP_PORT: u16 = 51331;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub struct DLiveClient<S = TcpStream> {
    stream: Pin<Box<Framed<S, DLiveCodec>>>,
}

impl DLiveClient<TcpStream> {
    /// Connects to the dLive without TLS.
    pub async fn new(addr: SocketAddr) -> io::Result<Self> {
        let stream = TcpStream::connect(addr).await?;

        Ok(Self::with_stream(stream))
    }
}

macro_rules! wait_until {
    ($self:expr, $pat:pat $(if $cond:expr)? => $res:expr) => {
        $self.wait_until(|m| match m {
            $pat $(if $cond)? => Some($res),
            _ => None,
        })
        .await?
    };
}

impl<S: AsyncRead + AsyncWrite> DLiveClient<S> {
    pub fn with_stream(stream: S) -> Self {
        Self {
            stream: Box::pin(Framed::new(stream, DLiveCodec::default())),
        }
    }

    async fn wait_until<T>(&mut self, mut f: impl FnMut(Message) -> Option<T>) -> Result<T> {
        timeout(REQUEST_TIMEOUT, async {
            while let Some(message) = self.stream.next().await.transpose()? {
                if let Some(res) = f(message.clone()) {
                    return Ok(res);
                }
                tracing::info!("Unexpected message: {message:?}")
            }
            anyhow::bail!("Unexpected end of stream");
        })
        .await?
    }

    pub async fn channel_names(&mut self, channels: &[Channel]) -> Result<Vec<ChannelName>> {
        for &channel in channels {
            self.stream
                .send(Message::GetChannelName { channel })
                .await?;
        }

        let mut names = Vec::new();
        for &channel in channels {
            let name = wait_until!(self, Message::ChannelName { channel: c, name } if c == channel => name);
            names.push(name);
        }
        Ok(names)
    }

    pub async fn send_level(&mut self, channel: Channel, send: Channel) -> Result<Level> {
        self.stream
            .send(Message::GetSendLevel { channel, send })
            .await?;

        let level = wait_until!(self, Message::SendLevel { channel: c, send: s, level } if c == channel && s == send => level);

        Ok(level)
    }

    pub async fn set_send_level(
        &mut self,
        channel: Channel,
        send: Channel,
        level: Level,
    ) -> Result<()> {
        self.stream
            .send(Message::SendLevel {
                channel,
                send,
                level,
            })
            .await?;

        Ok(())
    }

    pub async fn fader_level(&mut self, channel: Channel) -> Result<Level> {
        self.stream.send(Message::GetFaderLevel { channel }).await?;

        let level =
            wait_until!(self, Message::FaderLevel { channel: c, level } if c == channel => level);

        Ok(level)
    }

    pub async fn set_fader_level(&mut self, channel: Channel, level: Level) -> Result<()> {
        self.stream
            .send(Message::FaderLevel { channel, level })
            .await?;

        Ok(())
    }
}
