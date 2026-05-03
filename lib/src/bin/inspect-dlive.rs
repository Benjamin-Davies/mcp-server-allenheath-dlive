use std::{env::args, net::IpAddr};

use allenheath_dlive::client::DLiveClient;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().init();

    let args = args().collect::<Vec<_>>();
    anyhow::ensure!(args.len() == 2);
    let host = args[1].parse::<IpAddr>()?;

    let mut client = DLiveClient::new(host).await?;
    loop {
        let msg = client.recv().await?;
        println!("{msg:?}");
    }
}
