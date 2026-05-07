use std::{env::args, net::IpAddr};

use allenheath_dlive::{
    channels::{Channel, ChannelType},
    client::DLiveClient,
    messages::Message,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().init();

    let args = args().collect::<Vec<_>>();
    anyhow::ensure!(args.len() == 2);
    let host = args[1].parse::<IpAddr>()?;

    let mut client = DLiveClient::new(host).await?;
    for input in 1..=128 {
        let channel = Channel(ChannelType::Input, input);
        client.send(Message::GetChannelName { channel }).await?;
        client.send(Message::GetChannelColour { channel }).await?;
    }
    let mut incoming = client.incoming();
    loop {
        let msg = incoming.recv().await?;
        println!("{msg:?}");
    }
}
