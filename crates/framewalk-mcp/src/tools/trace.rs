use super::prelude::*;
use super::FULL_ONLY;

framewalk_tool_block! {
    router: catchpoint_tool_router,
    specs: CATCHPOINT_TOOL_SPECS,
    category: "catchpoints",
    profiles: FULL_ONLY,
    names: [
        catch_load,
        catch_unload,
        catch_assert,
        catch_exception,
        catch_handlers,
        catch_throw,
        catch_rethrow,
        catch_catch,
    ];
    items: {
        #[tool(description = "Catch shared library loads matching a regexp.")]
        async fn catch_load(
            &self,
            Parameters(args): Parameters<catchpoints::CatchLoadUnloadArgs>,
        ) -> Result<CallToolResult, McpError> {
            let mut cmd = MiCommand::new("catch-load");
            if args.temporary {
                cmd = cmd.option("t");
            }
            if args.disabled {
                cmd = cmd.option("d");
            }
            cmd = cmd.parameter(args.regexp);
            self.submit_as_tool_result(cmd).await
        }

        #[tool(description = "Catch shared library unloads matching a regexp.")]
        async fn catch_unload(
            &self,
            Parameters(args): Parameters<catchpoints::CatchLoadUnloadArgs>,
        ) -> Result<CallToolResult, McpError> {
            let mut cmd = MiCommand::new("catch-unload");
            if args.temporary {
                cmd = cmd.option("t");
            }
            if args.disabled {
                cmd = cmd.option("d");
            }
            cmd = cmd.parameter(args.regexp);
            self.submit_as_tool_result(cmd).await
        }

        #[tool(description = "Catch failed Ada assertions.")]
        async fn catch_assert(
            &self,
            Parameters(args): Parameters<catchpoints::CatchAdaExceptionArgs>,
        ) -> Result<CallToolResult, McpError> {
            let mut cmd = MiCommand::new("catch-assert");
            if let Some(c) = args.condition {
                cmd = cmd.option_with("c", c);
            }
            if args.disabled {
                cmd = cmd.option("d");
            }
            if args.temporary {
                cmd = cmd.option("t");
            }
            self.submit_as_tool_result(cmd).await
        }

        #[tool(description = "Catch Ada exceptions (optionally filtering by name or unhandled).")]
        async fn catch_exception(
            &self,
            Parameters(args): Parameters<catchpoints::CatchAdaExceptionArgs>,
        ) -> Result<CallToolResult, McpError> {
            let mut cmd = MiCommand::new("catch-exception");
            if let Some(c) = args.condition {
                cmd = cmd.option_with("c", c);
            }
            if args.disabled {
                cmd = cmd.option("d");
            }
            if args.temporary {
                cmd = cmd.option("t");
            }
            if let Some(name) = args.exception_name {
                cmd = cmd.option_with("e", name);
            }
            if args.unhandled {
                cmd = cmd.option("u");
            }
            self.submit_as_tool_result(cmd).await
        }

        #[tool(description = "Catch Ada exception handlers.")]
        async fn catch_handlers(
            &self,
            Parameters(args): Parameters<catchpoints::CatchAdaExceptionArgs>,
        ) -> Result<CallToolResult, McpError> {
            let mut cmd = MiCommand::new("catch-handlers");
            if let Some(c) = args.condition {
                cmd = cmd.option_with("c", c);
            }
            if args.disabled {
                cmd = cmd.option("d");
            }
            if args.temporary {
                cmd = cmd.option("t");
            }
            if let Some(name) = args.exception_name {
                cmd = cmd.option_with("e", name);
            }
            self.submit_as_tool_result(cmd).await
        }

        #[tool(description = "Catch C++ exception throws.")]
        async fn catch_throw(
            &self,
            Parameters(args): Parameters<catchpoints::CatchCppExceptionArgs>,
        ) -> Result<CallToolResult, McpError> {
            let mut cmd = MiCommand::new("catch-throw");
            if args.temporary {
                cmd = cmd.option("t");
            }
            if let Some(re) = args.regexp {
                cmd = cmd.option_with("r", re);
            }
            self.submit_as_tool_result(cmd).await
        }

        #[tool(description = "Catch C++ exception rethrows.")]
        async fn catch_rethrow(
            &self,
            Parameters(args): Parameters<catchpoints::CatchCppExceptionArgs>,
        ) -> Result<CallToolResult, McpError> {
            let mut cmd = MiCommand::new("catch-rethrow");
            if args.temporary {
                cmd = cmd.option("t");
            }
            if let Some(re) = args.regexp {
                cmd = cmd.option_with("r", re);
            }
            self.submit_as_tool_result(cmd).await
        }

        #[tool(description = "Catch C++ exception catches.")]
        async fn catch_catch(
            &self,
            Parameters(args): Parameters<catchpoints::CatchCppExceptionArgs>,
        ) -> Result<CallToolResult, McpError> {
            let mut cmd = MiCommand::new("catch-catch");
            if args.temporary {
                cmd = cmd.option("t");
            }
            if let Some(re) = args.regexp {
                cmd = cmd.option_with("r", re);
            }
            self.submit_as_tool_result(cmd).await
        }
    }
}

framewalk_tool_block! {
    router: tracepoint_tool_router,
    specs: TRACEPOINT_TOOL_SPECS,
    category: "tracepoints",
    profiles: FULL_ONLY,
    names: [
        trace_insert,
        trace_start,
        trace_stop,
        trace_status,
        trace_save,
        trace_list_variables,
        trace_define_variable,
        trace_find,
        trace_frame_collected,
    ];
    items: {
        #[tool(description = "Insert a tracepoint at a location. Returns the \
                           tracepoint number GDB assigned.")]
        async fn trace_insert(
            &self,
            Parameters(args): Parameters<tracepoints::TraceInsertArgs>,
        ) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(
                MiCommand::new("break-insert")
                    .option("a")
                    .parameter(args.location),
            )
            .await
        }

        #[tool(description = "Start collecting trace data.")]
        async fn trace_start(&self) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("trace-start"))
                .await
        }

        #[tool(description = "Stop collecting trace data.")]
        async fn trace_stop(&self) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("trace-stop"))
                .await
        }

        #[tool(description = "Query trace collection status.")]
        async fn trace_status(&self) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("trace-status"))
                .await
        }

        #[tool(description = "Save trace data to a file.")]
        async fn trace_save(
            &self,
            Parameters(args): Parameters<tracepoints::TraceSaveArgs>,
        ) -> Result<CallToolResult, McpError> {
            let mut cmd = MiCommand::new("trace-save");
            if args.remote {
                cmd = cmd.option("r");
            }
            if args.ctf {
                cmd = cmd.option("ctf");
            }
            cmd = cmd.parameter(args.filename);
            self.submit_as_tool_result(cmd).await
        }

        #[tool(description = "List trace state variables.")]
        async fn trace_list_variables(&self) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("trace-list-variables"))
                .await
        }

        #[tool(description = "Define a trace state variable.")]
        async fn trace_define_variable(
            &self,
            Parameters(args): Parameters<tracepoints::TraceDefineVariableArgs>,
        ) -> Result<CallToolResult, McpError> {
            let mut cmd = MiCommand::new("trace-define-variable").parameter(args.name);
            if let Some(val) = args.value {
                cmd = cmd.parameter(val);
            }
            self.submit_as_tool_result(cmd).await
        }

        #[tool(
            description = "Select a trace frame by various criteria (frame number, \
                           tracepoint number, PC address, source line, etc)."
        )]
        async fn trace_find(
            &self,
            Parameters(args): Parameters<tracepoints::TraceFindArgs>,
        ) -> Result<CallToolResult, McpError> {
            let mut cmd = MiCommand::new("trace-find");
            match args.mode {
                TraceFindMode::None => {
                    cmd = cmd.parameter("none");
                }
                TraceFindMode::FrameNumber { number } => {
                    cmd = cmd.parameter("frame-number").parameter(number.to_string());
                }
                TraceFindMode::TracepointNumber { number } => {
                    cmd = cmd
                        .parameter("tracepoint-number")
                        .parameter(number.to_string());
                }
                TraceFindMode::Pc { address } => {
                    cmd = cmd.parameter("pc").parameter(address);
                }
                TraceFindMode::PcInsideRange { start, end } => {
                    cmd = cmd
                        .parameter("pc-inside-range")
                        .parameter(start)
                        .parameter(end);
                }
                TraceFindMode::PcOutsideRange { start, end } => {
                    cmd = cmd
                        .parameter("pc-outside-range")
                        .parameter(start)
                        .parameter(end);
                }
                TraceFindMode::Line { location } => {
                    cmd = cmd.parameter("line").parameter(location);
                }
            }
            self.submit_as_tool_result(cmd).await
        }

        #[tool(description = "Return data collected at the current trace frame.")]
        async fn trace_frame_collected(
            &self,
            Parameters(args): Parameters<tracepoints::TraceFrameCollectedArgs>,
        ) -> Result<CallToolResult, McpError> {
            let mut cmd = MiCommand::new("trace-frame-collected");
            if let Some(pv) = args.var_print_values {
                cmd = cmd.option_with("var-print-values", pv.as_mi_arg());
            }
            if let Some(pv) = args.comp_print_values {
                cmd = cmd.option_with("comp-print-values", pv.as_mi_arg());
            }
            if let Some(rf) = args.registers_format {
                cmd = cmd.option_with("registers-format", rf.as_mi_arg());
            }
            if args.memory_contents {
                cmd = cmd.option("memory-contents");
            }
            self.submit_as_tool_result(cmd).await
        }
    }
}
