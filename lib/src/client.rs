//! https://www.allen-heath.com/content/uploads/2024/06/dLive-MIDI-Over-TCP-Protocol-V2.0.pdf

use std::{
    fmt::Debug,
    io,
    net::{IpAddr, SocketAddr},
    pin::Pin,
    time::Duration,
};

use anyhow::Result;
use futures::{SinkExt, Stream, StreamExt, stream::SplitSink};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpStream,
    sync::broadcast,
    task::JoinHandle,
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
    tx: Pin<Box<SplitSink<Framed<S, DLiveCodec>, Message>>>,
    _rx_task: JoinHandle<()>,
    rx_queue: broadcast::Receiver<Message>,
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

impl<S: AsyncRead + AsyncWrite + Debug + Send + 'static> DLiveClient<S> {
    #[tracing::instrument]
    pub fn with_stream(stream: S) -> Self {
        let (tx, rx) = Framed::new(stream, DLiveCodec::default()).split();

        let (rx_queue_tx, rx_queue) = broadcast::channel(128);
        let rx = Box::pin(rx);
        let _rx_task = tokio::task::spawn(rx_loop(rx, rx_queue_tx));

        Self {
            tx: Box::pin(tx),
            _rx_task,
            rx_queue,
        }
    }

    pub async fn send(&mut self, message: Message) -> anyhow::Result<()> {
        self.tx.send(message).await?;
        Ok(())
    }

    pub fn incoming(&self) -> broadcast::Receiver<Message> {
        self.rx_queue.resubscribe()
    }

    pub async fn recv(&self) -> anyhow::Result<Message> {
        let mut rx_queue = self.rx_queue.resubscribe();
        let message = rx_queue.recv().await?;
        Ok(message)
    }

    async fn wait_until<T>(&mut self, mut f: impl FnMut(Message) -> Option<T>) -> Result<T> {
        let mut rx_queue = self.rx_queue.resubscribe();
        timeout(REQUEST_TIMEOUT, async move {
            loop {
                let message = rx_queue.recv().await?;
                if let Some(res) = f(message) {
                    return Ok(res);
                }
                tracing::info!("Unexpected message: {message:?}")
            }
        })
        .await?
    }
}

#[tracing::instrument(skip(rx, queue_tx))]
async fn rx_loop<S: Stream<Item = anyhow::Result<Message>> + Debug + Send + Unpin + 'static>(
    mut rx: S,
    queue_tx: broadcast::Sender<Message>,
) {
    while let Some(message) = rx.next().await {
        match message {
            Ok(message) => {
                queue_tx.send(message).unwrap();
            }
            Err(err) => {
                tracing::error!("Error receiving message: {err}");
                for source in err.chain() {
                    tracing::error!("{source}");
                }
            }
        }
    }
}

impl<S: AsyncRead + AsyncWrite + Debug + Send + 'static> DLiveClient<S> {
    #[tracing::instrument]
    pub async fn channel_names(&mut self, channels: &[Channel]) -> Result<Vec<ChannelName>> {
        for &channel in channels {
            self.send(Message::GetChannelName { channel }).await?;
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
        self.send(Message::GetSendLevel { channel, send }).await?;

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
        self.send(Message::SendLevel {
            channel,
            send,
            level,
        })
        .await?;

        Ok(())
    }

    #[tracing::instrument]
    pub async fn fader_level(&mut self, channel: Channel) -> Result<Level> {
        self.send(Message::GetFaderLevel { channel }).await?;

        let level =
            wait_until!(self, Message::FaderLevel { channel: c, level } if c == channel => level);

        Ok(level)
    }

    #[tracing::instrument]
    pub async fn set_fader_level(&mut self, channel: Channel, level: Level) -> Result<()> {
        self.send(Message::FaderLevel { channel, level }).await?;

        Ok(())
    }
}
