use super::prelude::*;
use super::FULL_CORE;

framewalk_tool_block! {
    router: raw_tool_router,
    specs: RAW_TOOL_SPECS,
    category: "raw",
    profiles: FULL_CORE,
    names: [mi_raw_command];
    items: {
        #[tool(description = "Send a raw MI command and return the full result. \
                           Security: shell-adjacent commands \
                           (`-interpreter-exec console`, `-target-exec-command`, \
                           etc.) are rejected unless framewalk-mcp was started \
                           with --allow-shell.")]
        async fn mi_raw_command(
            &self,
            Parameters(args): Parameters<raw::MiRawCommandArgs>,
        ) -> Result<CallToolResult, McpError> {
            self.submit_validated_raw_command_as_tool_result(&args.command)
                .await
        }
    }
}
