use super::prelude::*;
use super::{FULL_CORE, FULL_ONLY};

framewalk_tool_block! {
    router: inspection_tool_router,
    specs: INSPECTION_TOOL_SPECS,
    category: "inspection",
    profiles: FULL_CORE,
    names: [backtrace, list_threads, select_frame, select_thread];
    items: {
        #[tool(description = "Return the current thread's call stack as a list of frames. \
                              Pass `limit: N` to cap the result at the N innermost frames.")]
        async fn backtrace(
            &self,
            Parameters(args): Parameters<stack::BacktraceArgs>,
        ) -> Result<CallToolResult, McpError> {
            let mut cmd = MiCommand::new("stack-list-frames");
            if let Some(limit) = args.limit {
                let high = limit.max(1) - 1;
                cmd = cmd.parameter("0".to_string()).parameter(high.to_string());
            }
            self.submit_as_tool_result(cmd).await
        }

        #[tool(description = "List threads in the target inferior. Pass a thread \
                           id to query a single thread, or omit to list all.")]
        async fn list_threads(
            &self,
            Parameters(args): Parameters<threads::ThreadInfoArgs>,
        ) -> Result<CallToolResult, McpError> {
            let mut cmd = MiCommand::new("thread-info");
            if let Some(tid) = args.thread_id {
                cmd = cmd.parameter(tid);
            }
            self.submit_as_tool_result(cmd).await
        }

        #[tool(description = "Select a stack frame by level within the current thread.")]
        async fn select_frame(
            &self,
            Parameters(args): Parameters<stack::SelectFrameArgs>,
        ) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(
                MiCommand::new("stack-select-frame").parameter(args.level.to_string()),
            )
            .await
        }

        #[tool(description = "Select a thread by id.")]
        async fn select_thread(
            &self,
            Parameters(args): Parameters<threads::ThreadSelectArgs>,
        ) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("thread-select").parameter(args.thread_id))
                .await
        }
    }
}

framewalk_tool_block! {
    router: stack_core_tool_router,
    specs: STACK_CORE_TOOL_SPECS,
    category: "stack",
    profiles: FULL_CORE,
    names: [frame_info, stack_depth, list_locals, list_arguments, list_variables];
    items: {
        #[tool(description = "Return info about the currently selected stack frame.")]
        async fn frame_info(&self) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("stack-info-frame"))
                .await
        }

        #[tool(description = "Return the depth (number of frames) of the current stack.")]
        async fn stack_depth(
            &self,
            Parameters(args): Parameters<stack::StackDepthArgs>,
        ) -> Result<CallToolResult, McpError> {
            let mut cmd = MiCommand::new("stack-info-depth");
            if let Some(max) = args.max_depth {
                cmd = cmd.parameter(max.to_string());
            }
            self.submit_as_tool_result(cmd).await
        }

        #[tool(description = "List local variables of the selected frame.")]
        async fn list_locals(
            &self,
            Parameters(args): Parameters<stack::StackListArgs>,
        ) -> Result<CallToolResult, McpError> {
            let mut cmd = MiCommand::new("stack-list-locals");
            if args.skip_unavailable {
                cmd = cmd.option("skip-unavailable");
            }
            cmd = cmd.parameter(args.print_values.as_mi_arg());
            self.submit_as_tool_result(cmd).await
        }

        #[tool(description = "List arguments of each frame.")]
        async fn list_arguments(
            &self,
            Parameters(args): Parameters<stack::StackListArgumentsArgs>,
        ) -> Result<CallToolResult, McpError> {
            let mut cmd = MiCommand::new("stack-list-arguments");
            if args.skip_unavailable {
                cmd = cmd.option("skip-unavailable");
            }
            cmd = cmd.parameter(args.print_values.as_mi_arg());
            if let (Some(lo), Some(hi)) = (args.low_frame, args.high_frame) {
                cmd = cmd.parameter(lo.to_string()).parameter(hi.to_string());
            }
            self.submit_as_tool_result(cmd).await
        }

        #[tool(description = "List all variables (locals + arguments) of the selected frame.")]
        async fn list_variables(
            &self,
            Parameters(args): Parameters<stack::StackListArgs>,
        ) -> Result<CallToolResult, McpError> {
            let mut cmd = MiCommand::new("stack-list-variables");
            if args.skip_unavailable {
                cmd = cmd.option("skip-unavailable");
            }
            cmd = cmd.parameter(args.print_values.as_mi_arg());
            self.submit_as_tool_result(cmd).await
        }
    }
}

framewalk_tool_block! {
    router: stack_extended_tool_router,
    specs: STACK_EXTENDED_TOOL_SPECS,
    category: "stack",
    profiles: FULL_ONLY,
    names: [enable_frame_filters];
    items: {
        #[tool(description = "Enable frame filter support in stack commands.")]
        async fn enable_frame_filters(&self) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("enable-frame-filters"))
                .await
        }
    }
}
