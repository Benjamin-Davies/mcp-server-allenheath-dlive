use std::{collections::BTreeMap, net::Ipv4Addr};

use allenheath_dlive::{
    channels::{Channel, ChannelType},
    client::DLiveClient,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut client = DLiveClient::new(Ipv4Addr::LOCALHOST.into()).await?;

    let channels = [
        (ChannelType::Input, 1..=128),
        (ChannelType::MonoAux, 1..=10),
        (ChannelType::StereoAux, 1..=12),
    ]
    .into_iter()
    .flat_map(|(t, ns)| ns.map(move |n| Channel(t, n)))
    .collect::<Vec<_>>();

    let names = client.channel_names(&channels).await?;

    let channel_names = channels.into_iter().zip(names).collect::<BTreeMap<_, _>>();
    tokio::fs::write(
        "channel-names.json",
        serde_json::to_string_pretty(&channel_names)?,
    )
    .await?;

    Ok(())
}
