use anyhow::Result;
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use tokio::sync::Mutex;

use crate::dlive::DLiveClient;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SumRequest {
    #[schemars(description = "the left hand side number")]
    pub a: i32,
    pub b: i32,
}

#[derive(Debug)]
pub struct DLiveHandler {
    state: Mutex<State>,
}

#[derive(Debug)]
struct State {
    client: Option<DLiveClient>,
}

impl DLiveHandler {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(State { client: None }),
        }
    }
}

impl State {
    async fn client(&mut self) -> &mut DLiveClient {
        if self.client.is_none() {
            let client = DLiveClient::new("10.6.103.10".parse().unwrap())
                .await
                .unwrap();
            self.client = Some(client);
        }
        self.client.as_mut().unwrap()
    }
}

#[tool_router(server_handler)]
impl DLiveHandler {
    #[tool(description = "Calculate the sum of two numbers")]
    fn sum(&self, Parameters(SumRequest { a, b }): Parameters<SumRequest>) -> String {
        (a + b).to_string()
    }

    #[tool(description = "Get the names of the inputs")]
    async fn list_inputs(&self) -> String {
        let mut state = self.state.lock().await;
        let client = state.client().await;
        match client.list_inputs().await {
            Ok(inputs) => inputs.join(","),
            Err(err) => err.to_string(),
        }
    }
}
