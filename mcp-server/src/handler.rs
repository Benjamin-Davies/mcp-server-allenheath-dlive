use ::rmcp::ServerHandler;
use anyhow::Result;
use rmcp::{
    ErrorData, RoleServer,
    handler::server::{tool::ToolCallContext, wrapper::Parameters},
    model::{
        CallToolRequestParams, CallToolResult, Implementation, ListToolsResult,
        PaginatedRequestParams, ServerCapabilities, ServerInfo, Tool,
    },
    schemars,
    service::RequestContext,
    tool, tool_router,
};
use tokio::sync::Mutex;

use allenheath_dlive::DLiveClient;

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

#[tool_router]
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

    // TODO: when increasing a level would hit a limit, turn all levels down then the master up or vice versa. Make sure to set a (documented) flag in the response to indicate to the agent that this happened and remove it from the instructions.
}

impl ServerHandler for DLiveHandler {
    fn get_info(&self) -> ServerInfo {
        let capabilities = ServerCapabilities::builder().enable_tools().build();
        let instructions = include_str!("instructions.md");
        let server_info = Implementation::from_build_env();

        ServerInfo::new(capabilities)
            .with_instructions(instructions)
            .with_server_info(server_info)
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult {
            tools: Self::tool_router().list_all(),
            meta: None,
            next_cursor: None,
        })
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        Self::tool_router().get(name).cloned()
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let tcc = ToolCallContext::new(self, request, context);
        Self::tool_router().call(tcc).await
    }
}
