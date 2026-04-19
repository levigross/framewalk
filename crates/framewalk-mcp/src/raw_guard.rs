//! Security guard for the `mi_raw_command` tool.
//!
//! This is the single MCP surface that lets an LLM send arbitrary MI
//! commands to GDB. GDB's MI grammar includes escape-hatches that pivot
//! from "just a debugger" into "arbitrary shell execution on the host":
//! `-interpreter-exec console "shell rm -rf /"`, `shell ...`, `!...`,
//! and `-target-exec-command` in some GDB builds.
//!
//! When `--allow-shell` is not set (the default), this guard uses an
//! **allowlist** of known MI command families rather than a denylist of
//! dangerous ones. Some entries are whole families (prefix matches like
//! `break-`), others are exact commands (`target-select`). This is
//! structurally more secure than a denylist: a new GDB command family
//! cannot bypass the guard unless it is explicitly added here.

/// Discoverability URI for the shipped raw-MI allowlist reference.
pub(crate) const ALLOWED_MI_REFERENCE_URI: &str = "framewalk://reference/allowed-mi";

/// Reason a raw-MI command was rejected by the guard. Contains enough
/// context for an MCP client's error message to tell the agent *why*
/// the call was denied, not just that it was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RawMiRejection {
    /// Empty command string.
    Empty,
    /// Command did not start with `-` followed by a letter, so it
    /// cannot be a valid MI command.
    NotAnMiCommand,
    /// The MI command's operation name is not in the allowlist of
    /// known-safe families. Requires `--allow-shell` to bypass.
    UnknownCommandFamily,
}

impl RawMiRejection {
    /// A short human-readable reason suitable for an MCP error reply.
    #[must_use]
    pub(crate) const fn reason(self) -> &'static str {
        match self {
            Self::Empty => "empty MI command",
            Self::NotAnMiCommand => {
                "raw input does not look like an MI command (must start with '-')"
            }
            Self::UnknownCommandFamily => {
                "MI command is not in the raw-MI allowlist; see \
                 framewalk://reference/allowed-mi or start framewalk-mcp with --allow-shell"
            }
        }
    }
}

/// How an allowlist entry matches the MI operation name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AllowlistMatch {
    Prefix,
    Exact,
}

/// A canonical raw-MI allowlist entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AllowlistEntry {
    pub(crate) operation: &'static str,
    pub(crate) match_kind: AllowlistMatch,
}

impl AllowlistEntry {
    #[must_use]
    pub(crate) fn matches(self, operation: &str) -> bool {
        match self.match_kind {
            AllowlistMatch::Prefix => operation.starts_with(self.operation),
            AllowlistMatch::Exact => operation == self.operation,
        }
    }
}

/// Canonical raw-MI allowlist.
///
/// Prefix entries model MI command families such as `break-*`; exact
/// entries are one-off commands that should not implicitly allow nearby
/// operations just because they share a textual prefix.
const ALLOWED_COMMANDS: &[AllowlistEntry] = &[
    AllowlistEntry {
        operation: "ada-",
        match_kind: AllowlistMatch::Prefix,
    },
    AllowlistEntry {
        operation: "add-",
        match_kind: AllowlistMatch::Prefix,
    },
    AllowlistEntry {
        operation: "break-",
        match_kind: AllowlistMatch::Prefix,
    },
    AllowlistEntry {
        operation: "catch-",
        match_kind: AllowlistMatch::Prefix,
    },
    AllowlistEntry {
        operation: "complete",
        match_kind: AllowlistMatch::Exact,
    },
    AllowlistEntry {
        operation: "data-",
        match_kind: AllowlistMatch::Prefix,
    },
    AllowlistEntry {
        operation: "dprintf-",
        match_kind: AllowlistMatch::Prefix,
    },
    AllowlistEntry {
        operation: "enable-",
        match_kind: AllowlistMatch::Prefix,
    },
    AllowlistEntry {
        operation: "environment-",
        match_kind: AllowlistMatch::Prefix,
    },
    AllowlistEntry {
        operation: "exec-",
        match_kind: AllowlistMatch::Prefix,
    },
    AllowlistEntry {
        operation: "file-",
        match_kind: AllowlistMatch::Prefix,
    },
    AllowlistEntry {
        operation: "gdb-",
        match_kind: AllowlistMatch::Prefix,
    },
    AllowlistEntry {
        operation: "inferior-",
        match_kind: AllowlistMatch::Prefix,
    },
    AllowlistEntry {
        operation: "info-",
        match_kind: AllowlistMatch::Prefix,
    },
    AllowlistEntry {
        operation: "list-",
        match_kind: AllowlistMatch::Prefix,
    },
    AllowlistEntry {
        operation: "remove-",
        match_kind: AllowlistMatch::Prefix,
    },
    AllowlistEntry {
        operation: "stack-",
        match_kind: AllowlistMatch::Prefix,
    },
    AllowlistEntry {
        operation: "symbol-",
        match_kind: AllowlistMatch::Prefix,
    },
    AllowlistEntry {
        operation: "target-attach",
        match_kind: AllowlistMatch::Exact,
    },
    AllowlistEntry {
        operation: "target-detach",
        match_kind: AllowlistMatch::Exact,
    },
    AllowlistEntry {
        operation: "target-disconnect",
        match_kind: AllowlistMatch::Exact,
    },
    AllowlistEntry {
        operation: "target-download",
        match_kind: AllowlistMatch::Exact,
    },
    AllowlistEntry {
        operation: "target-flash-erase",
        match_kind: AllowlistMatch::Exact,
    },
    AllowlistEntry {
        operation: "target-select",
        match_kind: AllowlistMatch::Exact,
    },
    AllowlistEntry {
        operation: "thread-",
        match_kind: AllowlistMatch::Prefix,
    },
    AllowlistEntry {
        operation: "trace-",
        match_kind: AllowlistMatch::Prefix,
    },
    AllowlistEntry {
        operation: "var-",
        match_kind: AllowlistMatch::Prefix,
    },
];

#[must_use]
pub(crate) const fn allowed_command_allowlist() -> &'static [AllowlistEntry] {
    ALLOWED_COMMANDS
}

/// Validate a raw MI command against framewalk's security guard.
///
/// `input` is the command-line form the LLM submitted, without the
/// leading token and without a trailing newline. `allow_shell` is the
/// server's `--allow-shell` configuration flag. Returns `Ok(())` if the
/// command is permitted.
///
/// Rules (applied in order):
/// 1. Reject empty input.
/// 2. Input must start with `-` followed by an ASCII letter (the MI
///    command prefix). Raw CLI commands are always rejected, even with
///    `--allow-shell`, because bypassing MI would defeat every other
///    framewalk guarantee.
/// 3. If `allow_shell` is `false`, the operation name must match one of
///    the canonical allowlist entries in [`allowed_command_allowlist`].
///    This is an allowlist approach: unknown families are rejected, so
///    new shell-adjacent GDB commands cannot bypass the guard.
pub(crate) fn validate_raw_mi_command(
    input: &str,
    allow_shell: bool,
) -> Result<(), RawMiRejection> {
    let operation = raw_mi_operation(input)?;

    if allow_shell {
        return Ok(());
    }

    let operation_lower = operation.to_ascii_lowercase();

    if ALLOWED_COMMANDS
        .iter()
        .copied()
        .any(|entry| entry.matches(&operation_lower))
    {
        return Ok(());
    }

    Err(RawMiRejection::UnknownCommandFamily)
}

/// Parse the operation name out of a raw MI command string.
///
/// Returns the original operation substring (without the leading `-`)
/// so callers can do exact or case-insensitive comparisons while still
/// preserving the original command text elsewhere.
pub(crate) fn raw_mi_operation(input: &str) -> Result<&str, RawMiRejection> {
    let trimmed = input.trim();

    if trimmed.is_empty() {
        return Err(RawMiRejection::Empty);
    }

    // An MI command starts with `-`. The second character must be a
    // letter (operation name) to distinguish from double-dash option
    // separators or stray punctuation.
    if !trimmed.starts_with('-') {
        return Err(RawMiRejection::NotAnMiCommand);
    }
    if !trimmed
        .chars()
        .nth(1)
        .is_some_and(|c| c.is_ascii_alphabetic())
    {
        return Err(RawMiRejection::NotAnMiCommand);
    }

    let operation = &trimmed[1..];
    Ok(operation
        .split_once(|c: char| c.is_ascii_whitespace())
        .map_or(operation, |(op, _)| op))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Permitted under default (allow_shell = false) ----

    #[test]
    fn allows_common_mi_commands() {
        for cmd in [
            "-gdb-version",
            "-list-features",
            "-break-insert main",
            "-exec-run",
            "-exec-continue",
            "-data-evaluate-expression x",
            "-var-create - * i",
            "-stack-list-frames",
            "-thread-info",
            "-file-exec-and-symbols /tmp/a.out",
            "-target-attach 1234",
            "-target-detach",
            "-target-select remote :3333",
            "-trace-find frame-number 0",
            "-catch-throw",
            "-environment-cd /tmp",
            "-symbol-info-functions",
            "-ada-task-info",
            "-add-inferior",
            "-remove-inferior 2",
            "-info-os",
            "-enable-timings yes",
            "-dprintf-insert main \"%d\" x",
            "-complete break ma",
        ] {
            assert!(
                validate_raw_mi_command(cmd, false).is_ok(),
                "expected {cmd:?} to be allowed"
            );
        }
    }

    #[test]
    fn strips_leading_trailing_whitespace() {
        assert!(validate_raw_mi_command("   -gdb-version   ", false).is_ok());
    }

    #[test]
    fn parses_operation_name() {
        assert_eq!(
            raw_mi_operation("  -TARGET-SELECT remote :3333  ").expect("operation should parse"),
            "TARGET-SELECT"
        );
    }

    // ---- Rejected under default ----

    #[test]
    fn rejects_empty() {
        assert_eq!(
            validate_raw_mi_command("", false).unwrap_err(),
            RawMiRejection::Empty
        );
        assert_eq!(
            validate_raw_mi_command("   \t  ", false).unwrap_err(),
            RawMiRejection::Empty
        );
    }

    #[test]
    fn rejects_cli_style_without_dash() {
        assert_eq!(
            validate_raw_mi_command("shell ls", false).unwrap_err(),
            RawMiRejection::NotAnMiCommand
        );
        assert_eq!(
            validate_raw_mi_command("!ls", false).unwrap_err(),
            RawMiRejection::NotAnMiCommand
        );
        assert_eq!(
            validate_raw_mi_command("info break", false).unwrap_err(),
            RawMiRejection::NotAnMiCommand
        );
    }

    #[test]
    fn rejects_dash_only_garbage() {
        assert_eq!(
            validate_raw_mi_command("-", false).unwrap_err(),
            RawMiRejection::NotAnMiCommand
        );
        assert_eq!(
            validate_raw_mi_command("--", false).unwrap_err(),
            RawMiRejection::NotAnMiCommand
        );
        assert_eq!(
            validate_raw_mi_command("-1234", false).unwrap_err(),
            RawMiRejection::NotAnMiCommand
        );
    }

    #[test]
    fn rejects_interpreter_exec() {
        for cmd in [
            "-interpreter-exec console \"shell rm -rf /\"",
            "-interpreter-exec console \"print x\"",
            "-interpreter-exec  console   \"shell ls\"",
            "-INTERPRETER-EXEC CONSOLE \"foo\"",
            "-interpreter-exec mi2 \"info break\"",
            "-interpreter-exec python \"import os\"",
        ] {
            let err = validate_raw_mi_command(cmd, false).unwrap_err();
            assert_eq!(
                err,
                RawMiRejection::UnknownCommandFamily,
                "expected rejection for {cmd:?}"
            );
        }
    }

    #[test]
    fn rejects_target_exec_command() {
        assert_eq!(
            validate_raw_mi_command("-target-exec-command \"rm /tmp/x\"", false).unwrap_err(),
            RawMiRejection::UnknownCommandFamily
        );
    }

    #[test]
    fn rejects_unknown_command_family() {
        assert_eq!(
            validate_raw_mi_command("-unknown-foo bar", false).unwrap_err(),
            RawMiRejection::UnknownCommandFamily
        );
    }

    #[test]
    fn exact_allowlist_entries_do_not_match_longer_names() {
        for cmd in [
            "-completex foo",
            "-target-selectx remote :3333",
            "-target-download-now",
        ] {
            assert_eq!(
                validate_raw_mi_command(cmd, false).unwrap_err(),
                RawMiRejection::UnknownCommandFamily,
                "expected exact-match rejection for {cmd:?}"
            );
        }
    }

    // ---- Permitted with --allow-shell ----

    #[test]
    fn allow_shell_permits_interpreter_exec() {
        assert!(validate_raw_mi_command("-interpreter-exec console \"info break\"", true).is_ok());
    }

    #[test]
    fn allow_shell_permits_unknown_families() {
        assert!(validate_raw_mi_command("-unknown-foo bar", true).is_ok());
    }

    #[test]
    fn allow_shell_does_not_bypass_mi_prefix_rule() {
        // Even with allow_shell, raw CLI without the `-` prefix is still
        // rejected. The guard is about command structure as well as
        // shell access.
        assert_eq!(
            validate_raw_mi_command("shell ls", true).unwrap_err(),
            RawMiRejection::NotAnMiCommand
        );
    }
}
