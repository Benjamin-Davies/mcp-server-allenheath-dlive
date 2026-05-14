use std::{fmt, ops::AddAssign, sync::Arc};

use anyhow::Context;
use rmcp::{
    ErrorData, Json, ServerHandler,
    handler::server::wrapper::Parameters,
    model::{Implementation, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
};
use tokio::sync::{Mutex, watch};

use allenheath_dlive::{
    channels::{Channel, ChannelName},
    client::DLiveClient,
    messages::Level,
};

use crate::args::{Args, ChannelConfig};

#[derive(Debug, serde::Serialize, schemars::JsonSchema)]
struct ListChannelsResponse {
    channels: Vec<ChannelDetails>,
}

#[derive(Debug, Clone, Copy, serde::Serialize, schemars::JsonSchema)]
struct ChannelDetails {
    internal: Channel,
    name: ChannelName,
}

#[derive(Debug, Clone, Copy, serde::Deserialize, schemars::JsonSchema)]
struct LevelDelta {
    /// The amount to adjust by. Positive for increase, negative for decrease.
    amount: f32,
    unit: LevelDeltaUnit,
}

#[derive(Debug, Clone, Copy, serde::Deserialize, schemars::JsonSchema)]
enum LevelDeltaUnit {
    Decibels,
    Percentage,
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

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct AdjustInputLevelRequest {
    input: ChannelName,
    mix: ChannelName,
    delta: LevelDelta,
}

#[derive(Debug, serde::Serialize, schemars::JsonSchema)]
struct InputLevelResponse {
    input: ChannelName,
    mix: ChannelName,
    level: Level,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct GetMixLevelRequest {
    mix: ChannelName,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct SetMixLevelRequest {
    mix: ChannelName,
    level: Level,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct AdjustMixLevelRequest {
    mix: ChannelName,
    delta: LevelDelta,
}

#[derive(Debug, serde::Serialize, schemars::JsonSchema)]
struct MixLevelResponse {
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
    config_rx: watch::Receiver<ChannelConfig>,
    client: Option<DLiveClient>,
    inputs: Vec<ChannelDetails>,
    mixes: Vec<ChannelDetails>,
}

impl DLiveHandler {
    #[tracing::instrument]
    pub fn new(args: Arc<Args>, config_rx: watch::Receiver<ChannelConfig>) -> Self {
        Self {
            state: Mutex::new(State {
                args,
                config_rx,
                client: None,
                inputs: Vec::new(),
                mixes: Vec::new(),
            }),
        }
    }
}

impl State {
    /// Clears the cached channel lists if the config has changed since the last call.
    fn invalidate_if_changed(&mut self) {
        if self.config_rx.has_changed().unwrap_or(false) {
            tracing::info!("Channel config changed, invalidating channel name caches");
            self.config_rx.mark_unchanged();
            self.inputs.clear();
            self.mixes.clear();
        }
    }

    #[tracing::instrument(skip(self))]
    async fn client(&mut self) -> anyhow::Result<&mut DLiveClient> {
        if self.client.is_none() {
            let client = DLiveClient::new(self.args.ip).await?;
            self.client = Some(client);
        }

        self.client.as_mut().context("Failed to connect to dLive")
    }

    #[tracing::instrument(skip(self))]
    async fn list_inputs(&mut self) -> anyhow::Result<&[ChannelDetails]> {
        self.invalidate_if_changed();
        if self.inputs.is_empty() {
            let inputs = self.config_rx.borrow().inputs.iter().collect::<Vec<_>>();

            let client = self.client().await?;
            let names = client.channel_names(&inputs).await?;

            self.inputs = inputs
                .into_iter()
                .zip(names)
                .map(|(id, name)| ChannelDetails { internal: id, name })
                .collect();
        }
        Ok(&self.inputs)
    }

    #[tracing::instrument(skip(self))]
    async fn list_mixes(&mut self) -> anyhow::Result<&[ChannelDetails]> {
        self.invalidate_if_changed();
        if self.mixes.is_empty() {
            let mixes = self.config_rx.borrow().mixes.iter().collect::<Vec<_>>();

            let client = self.client().await?;
            let names = client.channel_names(&mixes).await?;

            self.mixes = mixes
                .into_iter()
                .zip(names)
                .map(|(id, name)| ChannelDetails { internal: id, name })
                .collect();
        }
        Ok(&self.mixes)
    }

    #[tracing::instrument(skip(self))]
    async fn input_id(&mut self, name: ChannelName) -> anyhow::Result<Channel> {
        let inputs = self.list_inputs().await?;
        let details = inputs
            .iter()
            .find(|d| d.name == name)
            .context("could not find an input with that exact name")?;
        Ok(details.internal)
    }

    #[tracing::instrument(skip(self))]
    async fn mix_id(&mut self, name: ChannelName) -> anyhow::Result<Channel> {
        let mixes = self.list_mixes().await?;
        let details = mixes
            .iter()
            .find(|d| d.name == name)
            .context("could not find a mix with that exact name")?;
        Ok(details.internal)
    }
}

#[tool_router]
impl DLiveHandler {
    #[tool(description = "Get the names of the inputs.")]
    #[tracing::instrument(skip(self))]
    async fn list_inputs(&self) -> Result<Json<ListChannelsResponse>, ErrorData> {
        let mut state = self.state.lock().await;
        let inputs = state.list_inputs().await.map_err(internal_error)?;

        let response = ListChannelsResponse {
            channels: inputs.to_vec(),
        };
        Ok(Json(response))
    }

    #[tool(description = "Get the names of the mixes.")]
    #[tracing::instrument(skip(self))]
    async fn list_mixes(&self) -> Result<Json<ListChannelsResponse>, ErrorData> {
        let mut state = self.state.lock().await;
        let mixes = state.list_mixes().await.map_err(internal_error)?;

        let response = ListChannelsResponse {
            channels: mixes.to_vec(),
        };
        Ok(Json(response))
    }

    #[tool(description = "Gets the level of an input in a mix.")]
    #[tracing::instrument(skip(self))]
    async fn get_input_level(
        &self,
        Parameters(GetInputLevelRequest { input, mix }): Parameters<GetInputLevelRequest>,
    ) -> Result<Json<InputLevelResponse>, ErrorData> {
        let mut state = self.state.lock().await;
        let input_id = state.input_id(input).await.map_err(internal_error)?;
        let mix_id = state.mix_id(mix).await.map_err(internal_error)?;

        let client = state.client().await.map_err(internal_error)?;
        let level = client
            .send_level(input_id, mix_id)
            .await
            .map_err(internal_error)?;

        let response = InputLevelResponse { input, mix, level };
        Ok(Json(response))
    }

    #[tool(description = "Sets the level of an input in a mix.")]
    #[tracing::instrument(skip(self))]
    async fn set_input_level(
        &self,
        Parameters(SetInputLevelRequest { input, mix, level }): Parameters<SetInputLevelRequest>,
    ) -> Result<Json<InputLevelResponse>, ErrorData> {
        let mut state = self.state.lock().await;
        let input_id = state.input_id(input).await.map_err(internal_error)?;
        let mix_id = state.mix_id(mix).await.map_err(internal_error)?;

        let client = state.client().await.map_err(internal_error)?;
        client
            .set_send_level(input_id, mix_id, level)
            .await
            .map_err(internal_error)?;

        let response = InputLevelResponse { input, mix, level };
        Ok(Json(response))
    }

    #[tool(description = "Increases or decreases the level of an input in a mix.")]
    #[tracing::instrument(skip(self))]
    async fn adjust_input_level(
        &self,
        Parameters(AdjustInputLevelRequest { input, mix, delta }): Parameters<
            AdjustInputLevelRequest,
        >,
    ) -> Result<Json<InputLevelResponse>, ErrorData> {
        let mut state = self.state.lock().await;
        let input_id = state.input_id(input).await.map_err(internal_error)?;
        let mix_id = state.mix_id(mix).await.map_err(internal_error)?;

        let client = state.client().await.map_err(internal_error)?;
        let mut level = client
            .send_level(input_id, mix_id)
            .await
            .map_err(internal_error)?;
        level += delta;
        client
            .set_send_level(input_id, mix_id, level)
            .await
            .map_err(internal_error)?;

        let response = InputLevelResponse { input, mix, level };
        Ok(Json(response))
    }

    #[tool(description = "Gets the level of a mix.")]
    #[tracing::instrument(skip(self))]
    async fn get_mix_level(
        &self,
        Parameters(GetMixLevelRequest { mix }): Parameters<GetMixLevelRequest>,
    ) -> Result<Json<MixLevelResponse>, ErrorData> {
        let mut state = self.state.lock().await;
        let mix_id = state.mix_id(mix).await.map_err(internal_error)?;

        let client = state.client().await.map_err(internal_error)?;
        let level = client.fader_level(mix_id).await.map_err(internal_error)?;

        let response = MixLevelResponse { mix, level };
        Ok(Json(response))
    }

    #[tool(description = "Sets the level of a mix.")]
    #[tracing::instrument(skip(self))]
    async fn set_mix_level(
        &self,
        Parameters(SetMixLevelRequest { mix, level }): Parameters<SetMixLevelRequest>,
    ) -> Result<Json<MixLevelResponse>, ErrorData> {
        let mut state = self.state.lock().await;
        let mix_id = state.mix_id(mix).await.map_err(internal_error)?;

        let client = state.client().await.map_err(internal_error)?;
        client
            .set_fader_level(mix_id, level)
            .await
            .map_err(internal_error)?;

        let response = MixLevelResponse { mix, level };
        Ok(Json(response))
    }

    #[tool(description = "Increases or decreases the level of a mix.")]
    #[tracing::instrument(skip(self))]
    async fn adjust_mix_level(
        &self,
        Parameters(AdjustMixLevelRequest { mix, delta }): Parameters<AdjustMixLevelRequest>,
    ) -> Result<Json<MixLevelResponse>, ErrorData> {
        let mut state = self.state.lock().await;
        let mix_id = state.mix_id(mix).await.map_err(internal_error)?;

        let client = state.client().await.map_err(internal_error)?;
        let mut level = client.fader_level(mix_id).await.map_err(internal_error)?;
        level += delta;
        client
            .set_fader_level(mix_id, level)
            .await
            .map_err(internal_error)?;

        let response = MixLevelResponse { mix, level };
        Ok(Json(response))
    }
}

#[tool_handler]
impl ServerHandler for DLiveHandler {
    #[tracing::instrument(skip(self))]
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

impl LevelDelta {
    fn to_db(self) -> f32 {
        match self.unit {
            LevelDeltaUnit::Decibels => self.amount,
            LevelDeltaUnit::Percentage => 20.0 * (1.0 + self.amount / 100.0).log10(),
        }
    }
}

impl AddAssign<LevelDelta> for Level {
    fn add_assign(&mut self, rhs: LevelDelta) {
        let db = f32::from(*self) + rhs.to_db();
        *self = db.into();
    }
}
