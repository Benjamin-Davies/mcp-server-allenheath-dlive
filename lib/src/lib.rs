//! https://www.allen-heath.com/content/uploads/2024/06/dLive-MIDI-Over-TCP-Protocol-V2.0.pdf

use std::{net::IpAddr, pin::Pin};

use anyhow::{Context as _, Result};
use futures::{SinkExt, StreamExt};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpStream,
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

pub const DLIVE_TCP_PORT: u16 = 51325;

#[derive(Debug)]
pub struct DLiveClient<S = TcpStream> {
    stream: Pin<Box<Framed<S, DLiveCodec>>>,
}

impl DLiveClient<TcpStream> {
    /// Connects to the DLive mix-rack without TLS.
    pub async fn new(addr: IpAddr) -> Result<Self> {
        let stream = TcpStream::connect((addr, DLIVE_TCP_PORT))
            .await
            .with_context(|| format!("Failed to connect to dLive MixRack at {addr}"))?;

        Ok(Self::with_stream(stream))
    }
}

impl<S: AsyncRead + AsyncWrite> DLiveClient<S> {
    pub fn with_stream(stream: S) -> Self {
        Self {
            stream: Box::pin(Framed::new(stream, DLiveCodec::default())),
        }
    }

    pub async fn channel_names(&mut self, channels: &[Channel]) -> Result<Vec<ChannelName>> {
        for &channel in channels {
            self.stream
                .send(Message::GetChannelName { channel })
                .await?;
        }

        let mut names = Vec::new();
        for &channel in channels {
            let response = self
                .stream
                .next()
                .await
                .context("Unexpected end of stream")??;
            let Message::ChannelName {
                channel: res_channel,
                name,
            } = response
            else {
                anyhow::bail!("Unexpected message: {response:?}");
            };
            anyhow::ensure!(
                res_channel == channel,
                "Returned channel ({res_channel:?}) does not match request ({channel:?})"
            );
            names.push(name);
        }
        Ok(names)
    }

    pub async fn send_level(&mut self, channel: Channel, send: Channel) -> Result<Level> {
        self.stream
            .send(Message::GetSendLevel { channel, send })
            .await?;

        let response = self
            .stream
            .next()
            .await
            .context("Unexpected end of stream")??;
        let Message::SendLevel {
            channel: res_channel,
            send: res_send,
            level,
        } = response
        else {
            anyhow::bail!("Unexpected message: {response:?}");
        };
        anyhow::ensure!(
            res_channel == channel,
            "Returned channel does not match request"
        );
        anyhow::ensure!(res_send == send, "Returned send does not match request");

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
}
