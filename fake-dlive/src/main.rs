use std::{collections::BTreeMap, net::Ipv4Addr, sync::Arc};

use allenheath_dlive::{
    DLIVE_TCP_PORT,
    channels::Channel,
    codecs::DLiveCodec,
    messages::{Level, Message},
};
use futures::{SinkExt, TryStreamExt};
use tokio::{net::TcpListener, sync::Mutex};
use tokio_util::codec::Framed;

#[derive(Debug, Default)]
struct State {
    send_levels: BTreeMap<(Channel, Channel), Level>,
    // TODO: mix_levels: BTreeMap<Channel, Level>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().init();

    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, DLIVE_TCP_PORT)).await?;
    tracing::info!("Listening at {}", listener.local_addr()?);

    let state = Arc::new(Mutex::new(State::default()));

    loop {
        let (stream, _) = listener.accept().await?;

        let state = state.clone();
        tokio::spawn(async move {
            let mut stream = Framed::new(stream, DLiveCodec::default());

            while let Some(message) = stream.try_next().await? {
                match message {
                    Message::GetChannelName { channel } => {
                        let name = format!("@{channel}").parse().unwrap();
                        stream.send(Message::ChannelName { channel, name }).await?;
                    }
                    Message::ChannelName { .. } => {
                        // Ignore channel name changes. The MCP server will never send these.
                    }
                    Message::GetSendLevel { channel, send } => {
                        let state = state.lock().await;
                        let level = state
                            .send_levels
                            .get(&(channel, send))
                            .copied()
                            .unwrap_or(Level::ZERO);
                        stream
                            .send(Message::SendLevel {
                                channel,
                                send,
                                level,
                            })
                            .await?;
                    }
                    Message::SendLevel {
                        channel,
                        send,
                        level,
                    } => {
                        tracing::info!("Setting send level {channel} -> {send} to {level}");
                        let mut state = state.lock().await;
                        state.send_levels.insert((channel, send), level);
                    }
                }
            }

            Ok::<(), anyhow::Error>(())
        });
    }
}
