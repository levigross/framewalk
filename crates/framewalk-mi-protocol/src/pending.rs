//! Pending-command tracker with typed operations.

use std::collections::HashMap;

use framewalk_mi_codec::Token;

/// The category of a submitted MI command, so `route_done` can dispatch
/// the result payload to the correct state registry without a string
/// match. Typos become compile errors; routing is exhaustive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Operation {
    BreakInsert,
    BreakInfo,
    VarCreate { expression: Option<String> },
    VarUpdate,
    VarDelete { name: String },
    ListFeatures,
    ListTargetFeatures,
    TargetSelect,
    ExecRun,
    Other,
    Raw,
}

impl Operation {
    pub(crate) fn from_name(name: &str, parameters: &[String]) -> Self {
        match name {
            "break-insert" => Self::BreakInsert,
            "break-info" => Self::BreakInfo,
            "var-create" => Self::VarCreate {
                expression: parameters.last().cloned(),
            },
            "var-update" => Self::VarUpdate,
            "var-delete" => Self::VarDelete {
                name: parameters.first().cloned().unwrap_or_default(),
            },
            "list-features" => Self::ListFeatures,
            "list-target-features" => Self::ListTargetFeatures,
            "target-select" => Self::TargetSelect,
            "exec-run" => Self::ExecRun,
            _ => Self::Other,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PendingInfo {
    pub operation: Operation,
    pub operation_name: String,
}

impl PendingInfo {
    /// Whether this operation is an execution command whose `^error`
    /// reply leaves the target in an unknown state per the GDB manual.
    pub(crate) fn is_exec_command(&self) -> bool {
        self.operation_name.starts_with("exec-")
    }
}

#[derive(Debug, Default)]
pub(crate) struct PendingCommands {
    by_token: HashMap<Token, PendingInfo>,
}

impl PendingCommands {
    pub(crate) fn insert(&mut self, token: Token, info: PendingInfo) {
        self.by_token.insert(token, info);
    }

    pub(crate) fn remove(&mut self, token: Token) -> Option<PendingInfo> {
        self.by_token.remove(&token)
    }
}
