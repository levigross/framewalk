//! The rmcp `ServerHandler` that exposes framewalk as an MCP server.
//!
//! `FramewalkMcp` owns the server shell: transport handle, mode, and the
//! assembled `ToolRouter<Self>`. The semantic MI-first tool surface is
//! declared in `tools/*`; this file stays focused on construction,
//! shared submission helpers, and resource handling.

use std::sync::Arc;

use framewalk_mi_codec::MiCommand;
use framewalk_mi_protocol::CommandOutcome;
use framewalk_mi_transport::TransportHandle;
use rmcp::{
    handler::server::router::tool::ToolRouter,
    model::{
        CallToolResult, Implementation, ListResourcesResult, PaginatedRequestParam,
        ProtocolVersion, ReadResourceRequestParam, ReadResourceResult, ServerCapabilities,
        ServerInfo,
    },
    service::{RequestContext, RoleServer},
    tool_handler, ErrorData as McpError, ServerHandler,
};

use crate::config::Mode;
use crate::raw_guard::validate_raw_mi_command;
use crate::resources;
use crate::scheme::{self, SchemeHandle};
use crate::server_helpers::{
    format_outcome, raw_mi_rejection_to_mcp_error, remember_successful_raw_target_select,
    transport_error_to_mcp,
};
use crate::{tool_catalog, tools};

/// The rmcp `ServerHandler` framewalk-mcp exposes over stdio.
///
/// Cloned by rmcp for each tool dispatch; clones share the same
/// `Arc<TransportHandle>` so all tool calls hit the same GDB session.
#[derive(Clone)]
pub struct FramewalkMcp {
    transport: Arc<TransportHandle>,
    allow_shell: bool,
    mode: Mode,
    tool_router: ToolRouter<Self>,
}

impl std::fmt::Debug for FramewalkMcp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FramewalkMcp")
            .field("allow_shell", &self.allow_shell)
            .field("mode", &self.mode)
            .field("transport_pid", &self.transport.child_id())
            .finish_non_exhaustive()
    }
}

impl FramewalkMcp {
    /// Construct a new server bound to the given transport handle.
    ///
    /// `Mode::Full` exposes the complete MI-first surface plus
    /// `scheme_eval`. `Mode::Core` keeps the MI-first semantics but
    /// trims the advertised tool list to the high-frequency subset plus
    /// `mi_raw_command` and `scheme_eval`. `Mode::Scheme` advertises
    /// `scheme_eval` plus a small operator surface so agents still have
    /// observability and recovery primitives without the full MI tool
    /// catalog.
    #[must_use]
    pub fn new(
        transport: Arc<TransportHandle>,
        allow_shell: bool,
        mode: Mode,
        scheme: Arc<SchemeHandle>,
    ) -> Self {
        let tool_router = match mode {
            Mode::Full => {
                let mut router = tools::semantic_tool_router();
                router.add_route(scheme::scheme_eval_route(Arc::clone(&scheme)));
                router
            }
            Mode::Core => {
                let mut router = tools::semantic_tool_router();
                router.add_route(scheme::scheme_eval_route(Arc::clone(&scheme)));
                let allowed = tool_catalog::core_tool_names();
                tool_catalog::retain_only(router, &allowed)
            }
            Mode::Scheme => {
                let mut router = ToolRouter::new();
                router.merge(FramewalkMcp::operator_tool_router());
                router.add_route(scheme::scheme_eval_route(scheme));
                router
            }
        };

        Self {
            transport,
            allow_shell,
            mode,
            tool_router,
        }
    }

    /// Submit a structured MI command and translate the result into a
    /// consistent MCP tool result shape.
    pub(crate) async fn submit_as_tool_result(
        &self,
        command: MiCommand,
    ) -> Result<CallToolResult, McpError> {
        match self.transport.submit(command).await {
            Ok(outcome) => Ok(format_outcome(&outcome)),
            Err(err) => Err(transport_error_to_mcp(&err)),
        }
    }

    pub(crate) async fn submit_command(
        &self,
        command: MiCommand,
    ) -> Result<CommandOutcome, McpError> {
        self.transport
            .submit(command)
            .await
            .map_err(|err| transport_error_to_mcp(&err))
    }

    /// Submit a validated raw MI command, preserving the user-provided
    /// wire syntax verbatim.
    pub(crate) async fn submit_validated_raw_command_as_tool_result(
        &self,
        command: &str,
    ) -> Result<CallToolResult, McpError> {
        if let Err(rejection) = validate_raw_mi_command(command, self.allow_shell) {
            return Err(raw_mi_rejection_to_mcp_error(rejection));
        }

        match self.transport.submit_raw(command).await {
            Ok(outcome) => {
                remember_successful_raw_target_select(self.transport_handle(), command, &outcome);
                Ok(format_outcome(&outcome))
            }
            Err(err) => Err(transport_error_to_mcp(&err)),
        }
    }

    pub(crate) fn transport_handle(&self) -> &TransportHandle {
        &self.transport
    }

    /// Clone the `Arc<TransportHandle>` for a detached task (e.g. the
    /// post-connect vmlinux probe) that needs its own lifetime.
    pub(crate) fn transport_arc(&self) -> Arc<TransportHandle> {
        Arc::clone(&self.transport)
    }
}

#[tool_handler]
impl ServerHandler for FramewalkMcp {
    fn get_info(&self) -> ServerInfo {
        let instructions = match self.mode {
            Mode::Full => "\
                framewalk exposes the complete GDB/MI v3 debugging surface \
                as MCP tools. Read `framewalk://guide/getting-started` \
                first for the minimal workflow. Call `resources/list` to \
                discover topic guides (`framewalk://guide/*`), \
                per-category tool references (`framewalk://reference/*`), \
                and end-to-end workflow recipes (`framewalk://recipe/*`). \
                Shell-adjacent raw MI is blocked by default — see \
                `framewalk://guide/raw-mi` and \
                `framewalk://reference/allowed-mi` for the allowed surface."
                .to_string(),
            Mode::Core => "\
                framewalk in core mode keeps the MI-first debugging model \
                but advertises only the common day-to-day subset plus \
                `mi_raw_command` and `scheme_eval`. Start with \
                `framewalk://guide/getting-started`, then pull in \
                `framewalk://reference/*` guides as needed. If the core \
                subset is insufficient, `mi_raw_command` is the escape \
                hatch for allowed MI families (see \
                `framewalk://reference/allowed-mi`) and `scheme_eval` can \
                compose multi-step flows."
                .to_string(),
            Mode::Scheme => "\
                framewalk in Scheme mode exposes GDB/MI v3 primarily \
                through `scheme_eval`, plus operator escape hatches: \
                `interrupt_target`, `target_state`, `drain_events`, and \
                `reconnect_target`. Read `framewalk://guide/scheme` \
                first. The prelude provides wrappers: (load-file path), \
                (set-breakpoint loc), (run), (cont-and-wait), \
                (backtrace), (inspect expr), (wait-for-stop). See \
                `framewalk://reference/scheme` for the full prelude \
                function list and `framewalk://recipe/*` for worked \
                examples. Engine state persists across calls."
                .to_string(),
        };

        ServerInfo {
            protocol_version: ProtocolVersion::V_2025_06_18,
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
            server_info: Implementation {
                name: "framewalk-mcp".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                title: Some("framewalk — GDB/MI MCP server".to_string()),
                icons: None,
                website_url: None,
            },
            instructions: Some(instructions),
        }
    }

    async fn list_resources(
        &self,
        _params: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        Ok(resources::list_resources())
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        resources::read_resource(&request.uri)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn semantic_tool_specs_match_real_tool_surface() {
        let mut spec_names: Vec<&'static str> = crate::tools::all_tool_specs()
            .map(|spec| spec.name)
            .collect();
        spec_names.sort_unstable();

        let mut router_names: Vec<String> = crate::tools::semantic_tool_router()
            .list_all()
            .into_iter()
            .map(|tool| tool.name.into_owned())
            .collect();
        router_names.sort_unstable();

        let expected: Vec<String> = spec_names.into_iter().map(str::to_string).collect();
        assert_eq!(router_names, expected);
    }
}
