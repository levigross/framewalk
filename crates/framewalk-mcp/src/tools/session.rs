use super::prelude::*;
use super::FULL_CORE;
use crate::server_helpers::{collect_console_text_since, json_tool_result, outcome_to_json};
use framewalk_mi_protocol::CommandOutcome;

framewalk_tool_block! {
    router: session_tool_router,
    specs: SESSION_TOOL_SPECS,
    category: "session",
    profiles: FULL_CORE,
    names: [gdb_version, load_file, attach, detach];
    items: {
        #[tool(description = "Query GDB's version string. Confirms the GDB \
                           subprocess is alive and reports its banner directly \
                           in the tool result.")]
        async fn gdb_version(&self) -> Result<CallToolResult, McpError> {
            let cursor = self.transport_handle().event_cursor();
            let outcome = self.submit_command(MiCommand::new("gdb-version")).await?;
            let mut payload = outcome_to_json(&outcome);
            if let Some(version) = collect_console_text_since(self.transport_handle(), cursor) {
                if let Some(obj) = payload.as_object_mut() {
                    obj.insert("version".into(), serde_json::Value::String(version));
                }
            }
            Ok(json_tool_result(
                &payload,
                matches!(outcome, CommandOutcome::Error { .. }),
            ))
        }

        #[tool(description = "Load an executable and its symbol table. Must be \
                           called before `run` for source-level debugging. \
                           Pass an absolute path to the executable.")]
        async fn load_file(
            &self,
            Parameters(args): Parameters<file::FilePathArgs>,
        ) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("file-exec-and-symbols").parameter(args.path))
                .await
        }

        #[tool(description = "Attach to a running process by PID. The target \
                           process is paused on attachment; use `continue` to \
                           resume it.")]
        async fn attach(
            &self,
            Parameters(args): Parameters<target::AttachArgs>,
        ) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("target-attach").parameter(args.pid))
                .await
        }

        #[tool(description = "Detach from the currently attached process, leaving it running.")]
        async fn detach(&self) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("target-detach"))
                .await
        }
    }
}
