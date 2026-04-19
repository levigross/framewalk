use super::prelude::*;
use super::{FULL_CORE, FULL_ONLY};

framewalk_tool_block! {
    router: variables_core_tool_router,
    specs: VARIABLES_CORE_TOOL_SPECS,
    category: "variables",
    profiles: FULL_CORE,
    names: [
        inspect,
        watch_create,
        watch_list,
        watch_delete,
        var_list_children,
        var_evaluate_expression,
    ];
    items: {
        #[tool(description = "Evaluate an expression in the current frame and \
                           return its value. For one-shot reads; use \
                           `watch_create` for expressions you want to monitor.")]
        async fn inspect(
            &self,
            Parameters(args): Parameters<variables::InspectArgs>,
        ) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(
                MiCommand::new("data-evaluate-expression").parameter(args.expression),
            )
            .await
        }

        #[tool(description = "Create a GDB variable object that watches the \
                           given expression. Poll for changes with \
                           `watch_list` after each stop, read current \
                           values with `var_evaluate_expression`, and tear \
                           down with `watch_delete`. See \
                           `framewalk://guide/variables` for the full \
                           workflow and scope-lifetime semantics.")]
        async fn watch_create(
            &self,
            Parameters(args): Parameters<variables::WatchCreateArgs>,
        ) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(
                MiCommand::new("var-create")
                    .parameter("-")
                    .parameter("*")
                    .parameter(args.expression),
            )
            .await
        }

        #[tool(description = "List currently-active GDB variable objects.")]
        async fn watch_list(&self) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("var-update").parameter("*"))
                .await
        }

        #[tool(description = "Delete a variable object by name.")]
        async fn watch_delete(
            &self,
            Parameters(args): Parameters<variables::WatchDeleteArgs>,
        ) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("var-delete").parameter(args.name))
                .await
        }

        #[tool(description = "List children of a variable object (for expanding aggregates/arrays).")]
        async fn var_list_children(
            &self,
            Parameters(args): Parameters<variables::VarListChildrenArgs>,
        ) -> Result<CallToolResult, McpError> {
            let mut cmd = MiCommand::new("var-list-children");
            if let Some(pv) = args.print_values {
                cmd = cmd.parameter(pv.as_mi_arg());
            }
            cmd = cmd.parameter(args.name);
            if let (Some(from), Some(to)) = (args.from, args.to) {
                cmd = cmd.parameter(from.to_string()).parameter(to.to_string());
            }
            self.submit_as_tool_result(cmd).await
        }

        #[tool(description = "Evaluate a variable object and return its current value.")]
        async fn var_evaluate_expression(
            &self,
            Parameters(args): Parameters<variables::VarNameArgs>,
        ) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("var-evaluate-expression").parameter(args.name))
                .await
        }
    }
}

framewalk_tool_block! {
    router: variables_extended_tool_router,
    specs: VARIABLES_EXTENDED_TOOL_SPECS,
    category: "variables",
    profiles: FULL_ONLY,
    names: [
        var_assign,
        var_set_format,
        var_show_format,
        var_info_num_children,
        var_info_type,
        var_info_expression,
        var_info_path_expression,
        var_show_attributes,
        var_set_frozen,
        var_set_update_range,
        var_set_visualizer,
        enable_pretty_printing,
    ];
    items: {
        #[tool(description = "Assign a new value to a variable object.")]
        async fn var_assign(
            &self,
            Parameters(args): Parameters<variables::VarAssignArgs>,
        ) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(
                MiCommand::new("var-assign")
                    .parameter(args.name)
                    .parameter(args.expression),
            )
            .await
        }

        #[tool(description = "Set the display format of a variable object.")]
        async fn var_set_format(
            &self,
            Parameters(args): Parameters<variables::VarSetFormatArgs>,
        ) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(
                MiCommand::new("var-set-format")
                    .parameter(args.name)
                    .parameter(args.format.as_mi_arg()),
            )
            .await
        }

        #[tool(description = "Show the current display format of a variable object.")]
        async fn var_show_format(
            &self,
            Parameters(args): Parameters<variables::VarNameArgs>,
        ) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("var-show-format").parameter(args.name))
                .await
        }

        #[tool(description = "Return the number of children of a variable object.")]
        async fn var_info_num_children(
            &self,
            Parameters(args): Parameters<variables::VarNameArgs>,
        ) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("var-info-num-children").parameter(args.name))
                .await
        }

        #[tool(description = "Return the type of a variable object as a string.")]
        async fn var_info_type(
            &self,
            Parameters(args): Parameters<variables::VarNameArgs>,
        ) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("var-info-type").parameter(args.name))
                .await
        }

        #[tool(description = "Return the expression that a variable object represents.")]
        async fn var_info_expression(
            &self,
            Parameters(args): Parameters<variables::VarNameArgs>,
        ) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("var-info-expression").parameter(args.name))
                .await
        }

        #[tool(description = "Return the full path expression for a variable object \
                           (for use in other GDB commands).")]
        async fn var_info_path_expression(
            &self,
            Parameters(args): Parameters<variables::VarNameArgs>,
        ) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("var-info-path-expression").parameter(args.name))
                .await
        }

        #[tool(description = "Show whether a variable object is editable.")]
        async fn var_show_attributes(
            &self,
            Parameters(args): Parameters<variables::VarNameArgs>,
        ) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("var-show-attributes").parameter(args.name))
                .await
        }

        #[tool(description = "Freeze or thaw a variable object (frozen objects skip updates).")]
        async fn var_set_frozen(
            &self,
            Parameters(args): Parameters<variables::VarSetFrozenArgs>,
        ) -> Result<CallToolResult, McpError> {
            let flag = if args.frozen { "1" } else { "0" };
            self.submit_as_tool_result(
                MiCommand::new("var-set-frozen")
                    .parameter(args.name)
                    .parameter(flag),
            )
            .await
        }

        #[tool(description = "Set the child range that `-var-update` refreshes.")]
        async fn var_set_update_range(
            &self,
            Parameters(args): Parameters<variables::VarSetUpdateRangeArgs>,
        ) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(
                MiCommand::new("var-set-update-range")
                    .parameter(args.name)
                    .parameter(args.from.to_string())
                    .parameter(args.to.to_string()),
            )
            .await
        }

        #[tool(description = "Set a Python pretty-printer visualizer for a variable object.")]
        async fn var_set_visualizer(
            &self,
            Parameters(args): Parameters<variables::VarSetVisualizerArgs>,
        ) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(
                MiCommand::new("var-set-visualizer")
                    .parameter(args.name)
                    .parameter(args.visualizer),
            )
            .await
        }

        #[tool(description = "Enable Python pretty-printing for variable objects globally.")]
        async fn enable_pretty_printing(&self) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("enable-pretty-printing"))
                .await
        }
    }
}

framewalk_tool_block! {
    router: data_core_tool_router,
    specs: DATA_CORE_TOOL_SPECS,
    category: "data",
    profiles: FULL_CORE,
    names: [
        read_memory,
        disassemble,
        list_register_names,
        read_registers,
        list_changed_registers,
    ];
    items: {
        #[tool(description = "Read raw memory bytes from the target.")]
        async fn read_memory(
            &self,
            Parameters(args): Parameters<data::ReadMemoryArgs>,
        ) -> Result<CallToolResult, McpError> {
            let mut cmd = MiCommand::new("data-read-memory-bytes");
            if let Some(offset) = args.offset {
                cmd = cmd.option_with("o", offset.to_string());
            }
            cmd = cmd
                .parameter(args.address)
                .parameter(args.count.to_string());
            self.submit_as_tool_result(cmd).await
        }

        #[tool(description = "Disassemble a memory range.")]
        async fn disassemble(
            &self,
            Parameters(args): Parameters<data::DisassembleArgs>,
        ) -> Result<CallToolResult, McpError> {
            let mut cmd = MiCommand::new("data-disassemble")
                .option_with("s", args.start_addr)
                .option_with("e", args.end_addr);
            if let Some(opcodes) = args.opcodes {
                cmd = cmd.option_with("opcodes", opcodes.as_mi_arg());
            }
            if args.source {
                cmd = cmd.option("source");
            }
            cmd = cmd.parameter("--").parameter("0");
            self.submit_as_tool_result(cmd).await
        }

        #[tool(description = "List register names (all or specific register numbers).")]
        async fn list_register_names(
            &self,
            Parameters(args): Parameters<data::RegisterNamesArgs>,
        ) -> Result<CallToolResult, McpError> {
            let mut cmd = MiCommand::new("data-list-register-names");
            for r in &args.registers {
                cmd = cmd.parameter(r.to_string());
            }
            self.submit_as_tool_result(cmd).await
        }

        #[tool(description = "Read register values in the specified format.")]
        async fn read_registers(
            &self,
            Parameters(args): Parameters<data::RegisterValuesArgs>,
        ) -> Result<CallToolResult, McpError> {
            let mut cmd =
                MiCommand::new("data-list-register-values").parameter(args.format.as_mi_arg());
            for r in &args.registers {
                cmd = cmd.parameter(r.to_string());
            }
            self.submit_as_tool_result(cmd).await
        }

        #[tool(description = "List register numbers that changed since the last stop.")]
        async fn list_changed_registers(&self) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("data-list-changed-registers"))
                .await
        }
    }
}

framewalk_tool_block! {
    router: data_extended_tool_router,
    specs: DATA_EXTENDED_TOOL_SPECS,
    category: "data",
    profiles: FULL_ONLY,
    names: [write_memory, read_memory_deprecated];
    items: {
        #[tool(description = "Write hex-encoded bytes to target memory.")]
        async fn write_memory(
            &self,
            Parameters(args): Parameters<data::WriteMemoryArgs>,
        ) -> Result<CallToolResult, McpError> {
            let mut cmd = MiCommand::new("data-write-memory-bytes")
                .parameter(args.address)
                .parameter(args.contents);
            if let Some(count) = args.count {
                cmd = cmd.parameter(count.to_string());
            }
            self.submit_as_tool_result(cmd).await
        }

        #[tool(
            description = "Read target memory in a tabular format (DEPRECATED: prefer \
                           `read_memory` which uses `-data-read-memory-bytes`)."
        )]
        async fn read_memory_deprecated(
            &self,
            Parameters(args): Parameters<data::ReadMemoryDeprecatedArgs>,
        ) -> Result<CallToolResult, McpError> {
            let mut cmd = MiCommand::new("data-read-memory");
            if let Some(offset) = args.column_offset {
                cmd = cmd.option_with("o", offset.to_string());
            }
            cmd = cmd
                .parameter(args.address)
                .parameter(args.word_format.as_mi_arg())
                .parameter(args.word_size.to_string())
                .parameter(args.nr_rows.to_string())
                .parameter(args.nr_cols.to_string());
            if let Some(aschar) = args.aschar {
                cmd = cmd.parameter(aschar);
            }
            self.submit_as_tool_result(cmd).await
        }
    }
}
