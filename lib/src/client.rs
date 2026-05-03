//! https://www.allen-heath.com/content/uploads/2024/06/dLive-MIDI-Over-TCP-Protocol-V2.0.pdf

use std::{
    fmt::Debug,
    io,
    net::{IpAddr, SocketAddr},
    pin::Pin,
    task,
    time::Duration,
};

use anyhow::Result;
use futures::{SinkExt, StreamExt};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpStream,
    time::timeout,
};
use tokio_util::codec::Framed;

use crate::{
    DLIVE_FAKE_TCP_PORT, DLIVE_MIXRACK_TCP_PORT, DLIVE_SURFACE_TCP_PORT,
    channels::{Channel, ChannelName},
    codecs::DLiveCodec,
    messages::{Level, Message},
};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub struct DLiveClient<S = TcpStream> {
    pub stream: Pin<Box<Framed<S, DLiveCodec>>>,
}

impl DLiveClient<TcpStream> {
    /// Connects to the dLive without TLS.
    #[tracing::instrument]
    pub async fn new(ip_addr: IpAddr) -> anyhow::Result<Self> {
        for port in [
            DLIVE_MIXRACK_TCP_PORT,
            DLIVE_SURFACE_TCP_PORT,
            DLIVE_FAKE_TCP_PORT,
        ] {
            let addr = SocketAddr::new(ip_addr, port);
            match TcpStream::connect(addr).await {
                Ok(stream) => return Ok(Self::with_stream(stream)),
                Err(err) if err.kind() == io::ErrorKind::ConnectionRefused => {
                    tracing::warn!("No dLive at {addr}");
                    continue;
                }
                Err(err) => {
                    tracing::error!("{err}");
                    break;
                }
            }
        }

        anyhow::bail!("Failed to connect to dLive");
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

impl<S: AsyncRead + AsyncWrite + Debug> DLiveClient<S> {
    #[tracing::instrument]
    pub fn with_stream(stream: S) -> Self {
        Self {
            stream: Box::pin(Framed::new(stream, DLiveCodec::default())),
        }
    }

    fn drop_unread(&mut self) {
        let mut cx = task::Context::from_waker(task::Waker::noop());
        while let task::Poll::Ready(_) = self.stream.poll_next_unpin(&mut cx) {
            // Drop all unread messages
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

    #[tracing::instrument]
    pub async fn channel_names(&mut self, channels: &[Channel]) -> Result<Vec<ChannelName>> {
        self.drop_unread();

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

    #[tracing::instrument]
    pub async fn send_level(&mut self, channel: Channel, send: Channel) -> Result<Level> {
        self.drop_unread();

        self.stream
            .send(Message::GetSendLevel { channel, send })
            .await?;

        let level = wait_until!(self, Message::SendLevel { channel: c, send: s, level } if c == channel && s == send => level);

        Ok(level)
    }

    #[tracing::instrument]
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

    #[tracing::instrument]
    pub async fn fader_level(&mut self, channel: Channel) -> Result<Level> {
        self.drop_unread();

        self.stream.send(Message::GetFaderLevel { channel }).await?;

        let level =
            wait_until!(self, Message::FaderLevel { channel: c, level } if c == channel => level);

        Ok(level)
    }

    #[tracing::instrument]
    pub async fn set_fader_level(&mut self, channel: Channel, level: Level) -> Result<()> {
        self.stream
            .send(Message::FaderLevel { channel, level })
            .await?;

        Ok(())
    }
}
