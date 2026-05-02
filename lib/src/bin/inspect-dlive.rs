use std::{env::args, net::IpAddr};

use allenheath_dlive::DLiveClient;
use futures::StreamExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = args().collect::<Vec<_>>();
    anyhow::ensure!(args.len() == 3);
    let host = args[1].parse::<IpAddr>()?;
    let port = args[2].parse::<u16>()?;

    let mut client = DLiveClient::new((host, port).into()).await?;
    while let Some(msg) = client.stream.next().await {
        let msg = msg?;
        println!("{msg:?}");
    }

    Ok(())
}
