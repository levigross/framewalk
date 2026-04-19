//! Category-local tool definitions for the MCP surface.
//!
//! `server.rs` owns the server shell and lifecycle. The semantic tool
//! surface lives here, split by domain so no single file or macro block
//! has to carry the entire MI-first API.

use rmcp::handler::server::router::tool::ToolRouter;

pub(crate) const FULL_ONLY: u8 = 0b01;
pub(crate) const FULL_CORE: u8 = 0b11;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ToolSpec {
    pub(crate) name: &'static str,
    pub(crate) category: &'static str,
    pub(crate) profiles: u8,
}

impl ToolSpec {
    pub(crate) fn in_core(self) -> bool {
        self.profiles & FULL_CORE == FULL_CORE
    }
}

macro_rules! framewalk_tool_block {
    (
        router: $router:ident,
        specs: $specs:ident,
        category: $category:literal,
        profiles: $profiles:expr,
        names: [$($name:ident),* $(,)?];
        items: {
            $($items:item)*
        }
    ) => {
        pub(crate) const $specs: &[super::ToolSpec] = &[
            $(
                super::ToolSpec {
                    name: stringify!($name),
                    category: $category,
                    profiles: $profiles,
                },
            )*
        ];

        #[rmcp::tool_router(router = $router, vis = "pub(crate)")]
        impl crate::server::FramewalkMcp {
            $($items)*
        }
    };
}

mod prelude {
    pub(crate) use crate::server_helpers::{build_symbol_info_cmd, build_symbol_module_cmd};
    pub(crate) use crate::types::{
        breakpoints, catchpoints, context, data, execution, file, file_transfer, misc, raw,
        session, stack, support, symbol, target, threads, tracepoints, variables,
    };
    pub(crate) use framewalk_mi_codec::MiCommand;
    pub(crate) use framewalk_mi_protocol::mi_types::{TraceFindMode, WatchType};
    pub(crate) use rmcp::{
        handler::server::wrapper::Parameters, model::CallToolResult, tool, ErrorData as McpError,
    };
}

pub(crate) mod breakpoints;
pub(crate) mod environment;
pub(crate) mod execution;
pub(crate) mod inspection;
pub(crate) mod operator;
pub(crate) mod raw;
pub(crate) mod session;
pub(crate) mod symbols;
pub(crate) mod trace;
pub(crate) mod variables;

const ALL_TOOL_SPEC_GROUPS: &[&[ToolSpec]] = &[
    session::SESSION_TOOL_SPECS,
    operator::OPERATOR_TOOL_SPECS,
    execution::EXECUTION_CORE_TOOL_SPECS,
    execution::EXECUTION_EXTENDED_TOOL_SPECS,
    execution::EXECUTION_REVERSE_TOOL_SPECS,
    breakpoints::BREAKPOINTS_CORE_TOOL_SPECS,
    breakpoints::BREAKPOINTS_EXTENDED_TOOL_SPECS,
    inspection::INSPECTION_TOOL_SPECS,
    inspection::STACK_CORE_TOOL_SPECS,
    inspection::STACK_EXTENDED_TOOL_SPECS,
    variables::VARIABLES_CORE_TOOL_SPECS,
    variables::VARIABLES_EXTENDED_TOOL_SPECS,
    variables::DATA_CORE_TOOL_SPECS,
    variables::DATA_EXTENDED_TOOL_SPECS,
    raw::RAW_TOOL_SPECS,
    environment::CONTEXT_CORE_TOOL_SPECS,
    environment::CONTEXT_EXTENDED_TOOL_SPECS,
    environment::FILE_TOOL_SPECS,
    environment::TARGET_TOOL_SPECS,
    environment::FILE_TRANSFER_TOOL_SPECS,
    environment::SUPPORT_CORE_TOOL_SPECS,
    environment::SUPPORT_EXTENDED_TOOL_SPECS,
    environment::MISC_TOOL_SPECS,
    symbols::SYMBOL_CORE_TOOL_SPECS,
    symbols::SYMBOL_EXTENDED_TOOL_SPECS,
    trace::CATCHPOINT_TOOL_SPECS,
    trace::TRACEPOINT_TOOL_SPECS,
];

pub(crate) fn all_tool_specs() -> impl Iterator<Item = &'static ToolSpec> {
    ALL_TOOL_SPEC_GROUPS.iter().flat_map(|group| group.iter())
}

pub(crate) fn semantic_tool_router() -> ToolRouter<crate::server::FramewalkMcp> {
    let mut router = ToolRouter::new();
    router.merge(crate::server::FramewalkMcp::session_tool_router());
    router.merge(crate::server::FramewalkMcp::operator_tool_router());
    router.merge(crate::server::FramewalkMcp::execution_core_tool_router());
    router.merge(crate::server::FramewalkMcp::execution_extended_tool_router());
    router.merge(crate::server::FramewalkMcp::execution_reverse_tool_router());
    router.merge(crate::server::FramewalkMcp::breakpoints_core_tool_router());
    router.merge(crate::server::FramewalkMcp::breakpoints_extended_tool_router());
    router.merge(crate::server::FramewalkMcp::inspection_tool_router());
    router.merge(crate::server::FramewalkMcp::stack_core_tool_router());
    router.merge(crate::server::FramewalkMcp::stack_extended_tool_router());
    router.merge(crate::server::FramewalkMcp::variables_core_tool_router());
    router.merge(crate::server::FramewalkMcp::variables_extended_tool_router());
    router.merge(crate::server::FramewalkMcp::data_core_tool_router());
    router.merge(crate::server::FramewalkMcp::data_extended_tool_router());
    router.merge(crate::server::FramewalkMcp::raw_tool_router());
    router.merge(crate::server::FramewalkMcp::context_core_tool_router());
    router.merge(crate::server::FramewalkMcp::context_extended_tool_router());
    router.merge(crate::server::FramewalkMcp::file_tool_router());
    router.merge(crate::server::FramewalkMcp::target_tool_router());
    router.merge(crate::server::FramewalkMcp::file_transfer_tool_router());
    router.merge(crate::server::FramewalkMcp::support_core_tool_router());
    router.merge(crate::server::FramewalkMcp::support_extended_tool_router());
    router.merge(crate::server::FramewalkMcp::misc_tool_router());
    router.merge(crate::server::FramewalkMcp::symbol_core_tool_router());
    router.merge(crate::server::FramewalkMcp::symbol_extended_tool_router());
    router.merge(crate::server::FramewalkMcp::catchpoint_tool_router());
    router.merge(crate::server::FramewalkMcp::tracepoint_tool_router());
    router
}
