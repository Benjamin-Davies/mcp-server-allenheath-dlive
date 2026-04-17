use std::{collections::BTreeMap, fmt, sync::Arc};

use anyhow::Context;
use rmcp::{
    ErrorData, Json, ServerHandler,
    handler::server::wrapper::Parameters,
    model::{Implementation, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
};
use tokio::sync::Mutex;

use allenheath_dlive::{
    DLiveClient,
    channels::{Channel, ChannelName},
    messages::Level,
};

use crate::args::Args;

#[derive(Debug, serde::Serialize, schemars::JsonSchema)]
struct ListChannelsResponse {
    channels: Vec<ChannelDetails>,
}

#[derive(Debug, serde::Serialize, schemars::JsonSchema)]
struct ChannelDetails {
    id: Channel,
    name: ChannelName,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct GetInputLevelRequest {
    input: ChannelName,
    mix: ChannelName,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct SetInputLevelRequest {
    input: ChannelName,
    mix: ChannelName,
    level: Level,
}

#[derive(Debug, serde::Serialize, schemars::JsonSchema)]
struct InputLevelResponse {
    input: ChannelName,
    mix: ChannelName,
    level: Level,
}

#[derive(Debug)]
pub struct DLiveHandler {
    state: Mutex<State>,
}

#[derive(Debug)]
struct State {
    args: Arc<Args>,
    client: Option<DLiveClient>,
    inputs: BTreeMap<ChannelName, Channel>,
    mixes: BTreeMap<ChannelName, Channel>,
}

impl DLiveHandler {
    pub fn new(args: Arc<Args>) -> Self {
        Self {
            state: Mutex::new(State {
                args,
                client: None,
                inputs: BTreeMap::new(),
                mixes: BTreeMap::new(),
            }),
        }
    }
}

impl State {
    async fn client(&mut self) -> anyhow::Result<&mut DLiveClient> {
        if self.client.is_none() {
            let addr = self.args.ip;
            let client = DLiveClient::new(addr).await?;
            self.client = Some(client);
        }
        Ok(self.client.as_mut().unwrap())
    }
}

#[tool_router]
impl DLiveHandler {
    #[tool(description = "Get the names of the inputs. You must call this before any other tools.")]
    async fn list_inputs(&self) -> Result<Json<ListChannelsResponse>, ErrorData> {
        let mut state = self.state.lock().await;
        let inputs = state.args.inputs.iter().collect::<Vec<_>>();

        let client = state.client().await.map_err(internal_error)?;
        let names = client
            .channel_names(&inputs)
            .await
            .map_err(internal_error)?;

        state.inputs = names.iter().copied().zip(inputs.iter().copied()).collect();

        let response = ListChannelsResponse {
            channels: inputs
                .into_iter()
                .zip(names)
                .map(|(id, name)| ChannelDetails { id, name })
                .collect(),
        };
        Ok(Json(response))
    }

    #[tool(description = "Get the names of the mixes. You must call this before any other tools.")]
    async fn list_mixes(&self) -> Result<Json<ListChannelsResponse>, ErrorData> {
        let mut state = self.state.lock().await;
        let mixes = state.args.mixes.iter().collect::<Vec<_>>();

        let client = state.client().await.map_err(internal_error)?;
        let names = client.channel_names(&mixes).await.map_err(internal_error)?;

        state.mixes = names.iter().copied().zip(mixes.iter().copied()).collect();

        let response = ListChannelsResponse {
            channels: mixes
                .into_iter()
                .zip(names)
                .map(|(id, name)| ChannelDetails { id, name })
                .collect(),
        };
        Ok(Json(response))
    }

    #[tool(description = "Gets the level of an input.")]
    async fn get_input_level(
        &self,
        Parameters(GetInputLevelRequest { input, mix }): Parameters<GetInputLevelRequest>,
    ) -> Result<Json<InputLevelResponse>, ErrorData> {
        let mut state = self.state.lock().await;
        let input_id = *state
            .inputs
            .get(&input)
            .context("could not find input by name")
            .map_err(internal_error)?;
        let mix_id = *state
            .mixes
            .get(&mix)
            .context("could not find mix by name")
            .map_err(internal_error)?;

        let client = state.client().await.map_err(internal_error)?;
        let level = client
            .send_level(input_id, mix_id)
            .await
            .map_err(internal_error)?;

        let response = InputLevelResponse { input, mix, level };
        Ok(Json(response))
    }

    #[tool(description = "Sets the level of an input.")]
    async fn set_input_level(
        &self,
        Parameters(SetInputLevelRequest { input, mix, level }): Parameters<SetInputLevelRequest>,
    ) -> Result<Json<InputLevelResponse>, ErrorData> {
        let mut state = self.state.lock().await;
        let input_id = *state
            .inputs
            .get(&input)
            .context("could not find input by name")
            .map_err(internal_error)?;
        let mix_id = *state
            .mixes
            .get(&mix)
            .context("could not find mix by name")
            .map_err(internal_error)?;

        let client = state.client().await.map_err(internal_error)?;
        client
            .set_send_level(input_id, mix_id, level)
            .await
            .map_err(internal_error)?;

        let response = InputLevelResponse { input, mix, level };
        Ok(Json(response))
    }

    // TODO: when increasing a level would hit a limit, turn all levels down then the master up or vice versa. Make sure to set a (documented) flag in the response to indicate to the agent that this happened and remove it from the instructions.
}

#[tool_handler]
impl ServerHandler for DLiveHandler {
    fn get_info(&self) -> ServerInfo {
        let capabilities = ServerCapabilities::builder().enable_tools().build();
        let instructions = include_str!("instructions.md");
        let server_info = Implementation::from_build_env();

        ServerInfo::new(capabilities)
            .with_instructions(instructions)
            .with_server_info(server_info)
    }
}

fn internal_error(err: impl fmt::Display) -> ErrorData {
    ErrorData::internal_error(err.to_string(), None)
}
