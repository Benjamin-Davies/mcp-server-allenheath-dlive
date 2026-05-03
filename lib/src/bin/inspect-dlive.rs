use std::{env::args, net::IpAddr};

use allenheath_dlive::client::DLiveClient;
use futures::StreamExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = args().collect::<Vec<_>>();
    anyhow::ensure!(args.len() == 2);
    let host = args[1].parse::<IpAddr>()?;

    let mut client = DLiveClient::new(host).await?;
    while let Some(msg) = client.stream.next().await {
        let msg = msg?;
        println!("{msg:?}");
    }

    Ok(())
}
