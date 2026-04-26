use std::{collections::BTreeMap, net::Ipv4Addr, sync::Arc};

use allenheath_dlive::{
    DLIVE_FAKE_TCP_PORT,
    channels::{Channel, ChannelName},
    codecs::DLiveCodec,
    messages::{Level, Message},
};
use futures::{SinkExt, TryStreamExt};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::Mutex,
};
use tokio_util::codec::Framed;

#[derive(Debug, Default)]
struct State {
    channel_names: BTreeMap<Channel, ChannelName>,
    send_levels: BTreeMap<(Channel, Channel), Level>,
    fader_levels: BTreeMap<Channel, Level>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().init();

    let channel_names_json = tokio::fs::read_to_string("channel-names.json").await?;
    let channel_names = serde_json::from_str(&channel_names_json)?;

    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, DLIVE_FAKE_TCP_PORT)).await?;
    tracing::info!("Listening at {}", listener.local_addr()?);

    let state = Arc::new(Mutex::new(State {
        channel_names,
        ..Default::default()
    }));

    loop {
        let (stream, _) = listener.accept().await?;

        let state = state.clone();
        tokio::spawn(async move {
            match handle_connection(stream, state).await {
                Ok(()) => {
                    // Success, do nothing
                }
                Err(err) => {
                    tracing::error!("{err:?}");
                }
            }
        });
    }
}

async fn handle_connection(stream: TcpStream, state: Arc<Mutex<State>>) -> anyhow::Result<()> {
    let mut stream = Framed::new(stream, DLiveCodec::default());

    while let Some(message) = stream.try_next().await? {
        match message {
            Message::GetChannelName { channel } => {
                let state = state.lock().await;
                let name = state
                    .channel_names
                    .get(&channel)
                    .copied()
                    .unwrap_or_else(|| format!("@{channel}").parse().unwrap());
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
            Message::GetFaderLevel { channel } => {
                let state = state.lock().await;
                let level = state
                    .fader_levels
                    .get(&channel)
                    .copied()
                    .unwrap_or(Level::ZERO);
                stream.send(Message::FaderLevel { channel, level }).await?;
            }
            Message::FaderLevel { channel, level } => {
                tracing::info!("Setting fader level {channel} to {level}");
                let mut state = state.lock().await;
                state.fader_levels.insert(channel, level);
            }
        }
    }

    Ok(())
}
