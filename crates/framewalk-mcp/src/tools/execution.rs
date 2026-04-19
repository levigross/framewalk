use super::prelude::*;
use super::{FULL_CORE, FULL_ONLY};

framewalk_tool_block! {
    router: execution_core_tool_router,
    specs: EXECUTION_CORE_TOOL_SPECS,
    category: "execution",
    profiles: FULL_CORE,
    names: [run, cont, step, next, finish, interrupt, until];
    items: {
        #[tool(description = "Run the loaded program from the start. Returns \
                           immediately once GDB has started the target; the \
                           target then executes asynchronously. See \
                           `framewalk://guide/execution-model` for how to \
                           observe the eventual stop with `target_state`, \
                           `drain_events`, or Scheme `*-and-wait` helpers.")]
        async fn run(&self) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("exec-run")).await
        }

        #[tool(description = "Continue execution from the current stop.")]
        async fn cont(&self) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("exec-continue"))
                .await
        }

        #[tool(description = "Step into the next source line, descending into \
                           function calls if any.")]
        async fn step(&self) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("exec-step"))
                .await
        }

        #[tool(description = "Execute the next source line, stepping over \
                           function calls.")]
        async fn next(&self) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("exec-next"))
                .await
        }

        #[tool(description = "Run until the current function returns, then stop \
                           at the caller.")]
        async fn finish(&self) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("exec-finish"))
                .await
        }

        #[tool(description = "Interrupt all running target threads, causing the target to stop at the next safe point.")]
        async fn interrupt(&self) -> Result<CallToolResult, McpError> {
            self.submit_validated_raw_command_as_tool_result("-exec-interrupt --all")
                .await
        }

        #[tool(
            description = "Run until a location is reached, or until the next source line if omitted."
        )]
        async fn until(
            &self,
            Parameters(args): Parameters<execution::UntilArgs>,
        ) -> Result<CallToolResult, McpError> {
            let mut cmd = MiCommand::new("exec-until");
            if let Some(loc) = args.location {
                cmd = cmd.parameter(loc);
            }
            self.submit_as_tool_result(cmd).await
        }
    }
}

framewalk_tool_block! {
    router: execution_extended_tool_router,
    specs: EXECUTION_EXTENDED_TOOL_SPECS,
    category: "execution",
    profiles: FULL_ONLY,
    names: [step_instruction, next_instruction, return_from_function, jump];
    items: {
        #[tool(description = "Step one machine instruction (into calls).")]
        async fn step_instruction(&self) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("exec-step-instruction"))
                .await
        }

        #[tool(description = "Step one machine instruction (over calls).")]
        async fn next_instruction(&self) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("exec-next-instruction"))
                .await
        }

        #[tool(description = "Make the current function return immediately, optionally with a value.")]
        async fn return_from_function(
            &self,
            Parameters(args): Parameters<execution::ReturnArgs>,
        ) -> Result<CallToolResult, McpError> {
            let mut cmd = MiCommand::new("exec-return");
            if let Some(expr) = args.expression {
                cmd = cmd.parameter(expr);
            }
            self.submit_as_tool_result(cmd).await
        }

        #[tool(description = "Jump to a location without stopping. WARNING: skips intervening code.")]
        async fn jump(
            &self,
            Parameters(args): Parameters<execution::JumpArgs>,
        ) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("exec-jump").parameter(args.location))
                .await
        }
    }
}

framewalk_tool_block! {
    router: execution_reverse_tool_router,
    specs: EXECUTION_REVERSE_TOOL_SPECS,
    category: "execution",
    profiles: FULL_ONLY,
    names: [reverse_step, reverse_next, reverse_continue, reverse_finish];
    items: {
        #[tool(description = "Step backward to the previous source line. Requires \
                           reverse debugging (`target record-full`).")]
        async fn reverse_step(
            &self,
            Parameters(args): Parameters<execution::ReverseStepArgs>,
        ) -> Result<CallToolResult, McpError> {
            let mut cmd = MiCommand::new("exec-step");
            if args.reverse {
                cmd = cmd.option("reverse");
            }
            self.submit_as_tool_result(cmd).await
        }

        #[tool(description = "Step backward over the previous source line. Requires \
                           reverse debugging (`target record-full`).")]
        async fn reverse_next(
            &self,
            Parameters(args): Parameters<execution::ReverseStepArgs>,
        ) -> Result<CallToolResult, McpError> {
            let mut cmd = MiCommand::new("exec-next");
            if args.reverse {
                cmd = cmd.option("reverse");
            }
            self.submit_as_tool_result(cmd).await
        }

        #[tool(description = "Continue execution backward. Requires reverse \
                           debugging (`target record-full`).")]
        async fn reverse_continue(
            &self,
            Parameters(args): Parameters<execution::ReverseStepArgs>,
        ) -> Result<CallToolResult, McpError> {
            let mut cmd = MiCommand::new("exec-continue");
            if args.reverse {
                cmd = cmd.option("reverse");
            }
            self.submit_as_tool_result(cmd).await
        }

        #[tool(description = "Run backward until the current function's caller. \
                           Requires reverse debugging (`target record-full`).")]
        async fn reverse_finish(
            &self,
            Parameters(args): Parameters<execution::ReverseStepArgs>,
        ) -> Result<CallToolResult, McpError> {
            let mut cmd = MiCommand::new("exec-finish");
            if args.reverse {
                cmd = cmd.option("reverse");
            }
            self.submit_as_tool_result(cmd).await
        }
    }
}
