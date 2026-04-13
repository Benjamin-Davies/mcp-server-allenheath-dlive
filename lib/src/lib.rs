//! https://www.allen-heath.com/content/uploads/2024/06/dLive-MIDI-Over-TCP-Protocol-V2.0.pdf

use std::{net::IpAddr, pin::Pin};

use anyhow::{Context as _, Result};
use futures::{SinkExt, StreamExt};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpStream,
};
use tokio_util::codec::Framed;

use crate::{codecs::DLiveCodec, messages::Message};

pub use messages::Channel;

mod codecs;
#[allow(dead_code)]
mod faders;
mod messages;

const DLIVE_TCP_PORT: u16 = 51325;

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
            stream: Box::pin(Framed::new(stream, DLiveCodec)),
        }
    }

    pub async fn list_inputs(&mut self) -> Result<Vec<String>> {
        for n in 1..=128 {
            let channel = Channel::Input(n);
            self.stream
                .send(Message::GetChannelName { channel })
                .await?;
        }

        let mut names = Vec::new();
        for n in 0..=128 {
            let message = self
                .stream
                .next()
                .await
                .context("Unexpected end of stream")??;
            let Message::GetChannelNameResponse { channel, name } = message else {
                anyhow::bail!("Unexpected message: {message:?}");
            };
            anyhow::ensure!(
                channel == Channel::Input(n),
                "Returned channel does not match request"
            );
            names.push(name);
        }
        Ok(names)
    }
}
