use allenheath_dlive::DLiveClient;

#[tokio::main]
async fn main() {
    let mut client = DLiveClient::new("10.6.103.10".parse().unwrap())
        .await
        .unwrap();

    let inputs = client.list_inputs().await.unwrap();
    dbg!(inputs);
}
