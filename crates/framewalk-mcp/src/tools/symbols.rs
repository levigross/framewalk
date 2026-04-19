use super::prelude::*;
use super::{FULL_CORE, FULL_ONLY};

framewalk_tool_block! {
    router: symbol_core_tool_router,
    specs: SYMBOL_CORE_TOOL_SPECS,
    category: "symbols",
    profiles: FULL_CORE,
    names: [symbol_info_functions, symbol_info_types, symbol_info_variables];
    items: {
        #[tool(description = "List functions matching optional name/type filters.")]
        async fn symbol_info_functions(
            &self,
            Parameters(args): Parameters<symbol::SymbolInfoArgs>,
        ) -> Result<CallToolResult, McpError> {
            let cmd = build_symbol_info_cmd("symbol-info-functions", &args);
            self.submit_as_tool_result(cmd).await
        }

        #[tool(description = "List types matching optional name filter.")]
        async fn symbol_info_types(
            &self,
            Parameters(args): Parameters<symbol::SymbolInfoArgs>,
        ) -> Result<CallToolResult, McpError> {
            let cmd = build_symbol_info_cmd("symbol-info-types", &args);
            self.submit_as_tool_result(cmd).await
        }

        #[tool(description = "List variables matching optional name/type filters.")]
        async fn symbol_info_variables(
            &self,
            Parameters(args): Parameters<symbol::SymbolInfoArgs>,
        ) -> Result<CallToolResult, McpError> {
            let cmd = build_symbol_info_cmd("symbol-info-variables", &args);
            self.submit_as_tool_result(cmd).await
        }
    }
}

framewalk_tool_block! {
    router: symbol_extended_tool_router,
    specs: SYMBOL_EXTENDED_TOOL_SPECS,
    category: "symbols",
    profiles: FULL_ONLY,
    names: [
        symbol_info_modules,
        symbol_info_module_functions,
        symbol_info_module_variables,
        symbol_list_lines,
    ];
    items: {
        #[tool(description = "List Fortran modules matching optional name filter.")]
        async fn symbol_info_modules(
            &self,
            Parameters(args): Parameters<symbol::SymbolInfoArgs>,
        ) -> Result<CallToolResult, McpError> {
            let cmd = build_symbol_info_cmd("symbol-info-modules", &args);
            self.submit_as_tool_result(cmd).await
        }

        #[tool(description = "List functions defined in Fortran modules.")]
        async fn symbol_info_module_functions(
            &self,
            Parameters(args): Parameters<symbol::SymbolModuleArgs>,
        ) -> Result<CallToolResult, McpError> {
            let cmd = build_symbol_module_cmd("symbol-info-module-functions", &args);
            self.submit_as_tool_result(cmd).await
        }

        #[tool(description = "List variables defined in Fortran modules.")]
        async fn symbol_info_module_variables(
            &self,
            Parameters(args): Parameters<symbol::SymbolModuleArgs>,
        ) -> Result<CallToolResult, McpError> {
            let cmd = build_symbol_module_cmd("symbol-info-module-variables", &args);
            self.submit_as_tool_result(cmd).await
        }

        #[tool(description = "List line number entries for a source file.")]
        async fn symbol_list_lines(
            &self,
            Parameters(args): Parameters<symbol::SymbolListLinesArgs>,
        ) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("symbol-list-lines").parameter(args.filename))
                .await
        }
    }
}
