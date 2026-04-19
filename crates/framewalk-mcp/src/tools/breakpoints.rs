use super::prelude::*;
use super::{FULL_CORE, FULL_ONLY};

framewalk_tool_block! {
    router: breakpoints_core_tool_router,
    specs: BREAKPOINTS_CORE_TOOL_SPECS,
    category: "breakpoints",
    profiles: FULL_CORE,
    names: [
        set_breakpoint,
        list_breakpoints,
        delete_breakpoint,
        enable_breakpoint,
        disable_breakpoint,
        break_condition,
        break_after,
        break_info,
        set_watchpoint,
    ];
    items: {
        #[tool(description = "Insert a breakpoint. `location` can be a function \
                           name (`main`), a file:line (`hello.c:42`), or a raw \
                           address (`*0x400500`). Returns the breakpoint id \
                           GDB assigned.")]
        async fn set_breakpoint(
            &self,
            Parameters(args): Parameters<breakpoints::SetBreakpointArgs>,
        ) -> Result<CallToolResult, McpError> {
            let mut cmd = MiCommand::new("break-insert");
            if args.temporary {
                cmd = cmd.option("t");
            }
            if let Some(condition) = args.condition {
                cmd = cmd.option_with("c", condition);
            }
            cmd = cmd.parameter(args.location);
            self.submit_as_tool_result(cmd).await
        }

        #[tool(description = "List all currently-defined breakpoints.")]
        async fn list_breakpoints(&self) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("break-list"))
                .await
        }

        #[tool(description = "Delete a breakpoint by id.")]
        async fn delete_breakpoint(
            &self,
            Parameters(args): Parameters<breakpoints::BreakpointIdArgs>,
        ) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("break-delete").parameter(args.id))
                .await
        }

        #[tool(description = "Enable a disabled breakpoint by id.")]
        async fn enable_breakpoint(
            &self,
            Parameters(args): Parameters<breakpoints::BreakpointIdArgs>,
        ) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("break-enable").parameter(args.id))
                .await
        }

        #[tool(description = "Disable a breakpoint by id without deleting it.")]
        async fn disable_breakpoint(
            &self,
            Parameters(args): Parameters<breakpoints::BreakpointIdArgs>,
        ) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("break-disable").parameter(args.id))
                .await
        }

        #[tool(description = "Set or modify a breakpoint's condition expression.")]
        async fn break_condition(
            &self,
            Parameters(args): Parameters<breakpoints::BreakConditionArgs>,
        ) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(
                MiCommand::new("break-condition")
                    .parameter(args.id)
                    .parameter(args.condition),
            )
            .await
        }

        #[tool(description = "Set a breakpoint's ignore count (skip the next N hits).")]
        async fn break_after(
            &self,
            Parameters(args): Parameters<breakpoints::BreakAfterArgs>,
        ) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(
                MiCommand::new("break-after")
                    .parameter(args.id)
                    .parameter(args.count.to_string()),
            )
            .await
        }

        #[tool(description = "Show info for a single breakpoint by id.")]
        async fn break_info(
            &self,
            Parameters(args): Parameters<breakpoints::BreakpointIdArgs>,
        ) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("break-info").parameter(args.id))
                .await
        }

        #[tool(description = "Set a watchpoint on an expression.")]
        async fn set_watchpoint(
            &self,
            Parameters(args): Parameters<breakpoints::SetWatchpointArgs>,
        ) -> Result<CallToolResult, McpError> {
            let mut cmd = MiCommand::new("break-watch");
            match args.watch_type {
                WatchType::Read => {
                    cmd = cmd.option("r");
                }
                WatchType::Access => {
                    cmd = cmd.option("a");
                }
                WatchType::Write => {}
            }
            cmd = cmd.parameter(args.expression);
            self.submit_as_tool_result(cmd).await
        }
    }
}

framewalk_tool_block! {
    router: breakpoints_extended_tool_router,
    specs: BREAKPOINTS_EXTENDED_TOOL_SPECS,
    category: "breakpoints",
    profiles: FULL_ONLY,
    names: [break_commands, break_passcount, dprintf_insert];
    items: {
        #[tool(description = "Set CLI commands to execute when a breakpoint is hit.")]
        async fn break_commands(
            &self,
            Parameters(args): Parameters<breakpoints::BreakCommandsArgs>,
        ) -> Result<CallToolResult, McpError> {
            let mut cmd = MiCommand::new("break-commands").parameter(args.id);
            for c in &args.commands {
                cmd = cmd.parameter(format!("\"{c}\""));
            }
            self.submit_as_tool_result(cmd).await
        }

        #[tool(description = "Set a tracepoint's passcount (auto-stop after N collections).")]
        async fn break_passcount(
            &self,
            Parameters(args): Parameters<breakpoints::BreakPasscountArgs>,
        ) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(
                MiCommand::new("break-passcount")
                    .parameter(args.id)
                    .parameter(args.passcount.to_string()),
            )
            .await
        }

        #[tool(
            description = "Insert a dynamic printf breakpoint that prints at a location \
                           without stopping (unless a condition fails)."
        )]
        async fn dprintf_insert(
            &self,
            Parameters(args): Parameters<breakpoints::DprintfInsertArgs>,
        ) -> Result<CallToolResult, McpError> {
            let mut cmd = MiCommand::new("dprintf-insert");
            if args.temporary {
                cmd = cmd.option("t");
            }
            if let Some(condition) = args.condition {
                cmd = cmd.option_with("c", condition);
            }
            if let Some(count) = args.ignore_count {
                cmd = cmd.option_with("i", count.to_string());
            }
            if let Some(tid) = args.thread_id {
                cmd = cmd.option_with("p", tid);
            }
            cmd = cmd.parameter(args.location).parameter(args.format);
            for a in &args.args {
                cmd = cmd.parameter(a.clone());
            }
            self.submit_as_tool_result(cmd).await
        }
    }
}
