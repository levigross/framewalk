use super::prelude::*;
use super::FULL_CORE;

use crate::server_helpers::{
    drain_observed_events, json_tool_result, observe_target_state, outcome_to_json,
};
use framewalk_mi_protocol::CommandOutcome;

framewalk_tool_block! {
    router: operator_tool_router,
    specs: OPERATOR_TOOL_SPECS,
    category: "operator",
    profiles: FULL_CORE,
    names: [interrupt_target, target_state, drain_events, reconnect_target];
    items: {
        #[tool(description = "Interrupt all running target threads immediately, even in scheme mode.")]
        async fn interrupt_target(&self) -> Result<CallToolResult, McpError> {
            self.submit_validated_raw_command_as_tool_result("-exec-interrupt --all")
                .await
        }

        #[tool(description = "Return framewalk's locally observed target state without querying GDB.")]
        async fn target_state(&self) -> Result<CallToolResult, McpError> {
            let observed = observe_target_state(self.transport_handle());
            Ok(json_tool_result(
                &serde_json::to_value(observed).unwrap_or_else(|_| serde_json::json!({"state":"unknown"})),
                false,
            ))
        }

        #[tool(description = "Drain retained async and stream events after a cursor, without sending any MI command.")]
        async fn drain_events(
            &self,
            Parameters(args): Parameters<session::DrainEventsArgs>,
        ) -> Result<CallToolResult, McpError> {
            let observed = drain_observed_events(self.transport_handle(), args.cursor.unwrap_or(0));
            Ok(json_tool_result(
                &serde_json::to_value(observed)
                    .unwrap_or_else(|_| serde_json::json!({"cursor": self.transport_handle().event_cursor(), "events": []})),
                false,
            ))
        }

        #[tool(description = "Disconnect and reconnect to the most recently selected remote target, preserving the current GDB session state.")]
        async fn reconnect_target(&self) -> Result<CallToolResult, McpError> {
            let Some(selection) = self.transport_handle().last_target_selection_command() else {
                return Err(McpError::invalid_params(
                    "no previous successful -target-select command is available to reconnect",
                    None,
                ));
            };

            let disconnect = Some(self.submit_command(MiCommand::new("target-disconnect")).await?);

            let reconnect = self.transport_handle().submit_raw(&selection).await
                .map_err(|err| crate::server_helpers::transport_error_to_mcp(&err))?;

            crate::server_helpers::remember_successful_raw_target_select(
                self.transport_handle(),
                &selection,
                &reconnect,
            );

            let payload = serde_json::json!({
                "disconnect": disconnect.as_ref().map(outcome_to_json),
                "reconnect": outcome_to_json(&reconnect),
            });
            Ok(json_tool_result(
                &payload,
                matches!(reconnect, CommandOutcome::Error { .. }),
            ))
        }
    }
}
