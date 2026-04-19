//! Target execution state.
//!
//! The protocol layer tracks target state as a small state machine updated
//! by `*running` / `*stopped` async events and reset to
//! [`TargetState::Unknown`] whenever an execution command reports an error
//! — per the GDB manual, "there's no guarantee that whenever an MI command
//! reports an error, GDB or the target are in any specific state."

use framewalk_mi_codec::Value;

use crate::results_view::{get_i32, get_str, get_string};
use crate::state::threads::ThreadId;

/// The execution state of the target inferior as observed by the protocol
/// layer.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum TargetState {
    /// Initial state and the state the protocol re-enters after any
    /// `^error` reply to an execution command.
    #[default]
    Unknown,
    /// The target is executing. `thread` identifies which thread (if `Some`)
    /// or whether all threads are running (`None` — the all-thread default
    /// in all-stop mode).
    Running { thread: Option<ThreadId> },
    /// The target is halted at `thread` with the raw stop-reason payload
    /// from the `*stopped` record. Callers can inspect the
    /// [`StoppedReason`] to branch on breakpoint vs signal vs exit.
    Stopped {
        thread: Option<ThreadId>,
        reason: Option<StoppedReason>,
    },
    /// The target exited. `exit_code` is the numeric code if GDB reported
    /// one (GDB sends it as a string like `"0"` or `"01"`; we parse it).
    Exited { exit_code: Option<i32> },
}

impl TargetState {
    /// `true` if the target is currently running (or "unknown" state's
    /// conservative answer of `false`).
    #[must_use]
    pub const fn is_running(&self) -> bool {
        matches!(self, Self::Running { .. })
    }

    /// `true` if the target is currently stopped.
    #[must_use]
    pub const fn is_stopped(&self) -> bool {
        matches!(self, Self::Stopped { .. })
    }

    /// `true` if the target has exited.
    #[must_use]
    pub const fn is_exited(&self) -> bool {
        matches!(self, Self::Exited { .. })
    }

    /// Reset to [`TargetState::Unknown`]. Called by the Connection on
    /// any `^error` reply to an execution-category command per the GDB
    /// manual's stated guarantee.
    pub(crate) fn mark_unknown(&mut self) {
        *self = Self::Unknown;
    }
}

/// Parsed `reason` field from a `*stopped` async record. Covers the
/// reasons the GDB manual enumerates; unknown reasons arrive as
/// [`StoppedReason::Other`] with the raw string so forward-compat is safe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoppedReason {
    BreakpointHit {
        bkptno: Option<String>,
    },
    WatchpointTrigger,
    FunctionFinished,
    LocationReached,
    EndSteppingRange,
    ExitedNormally,
    Exited {
        exit_code: Option<i32>,
    },
    ExitedSignalled {
        signal_name: Option<String>,
    },
    SignalReceived {
        signal_name: Option<String>,
    },
    Fork,
    Vfork,
    Exec,
    SyscallEntry {
        syscall_name: Option<String>,
    },
    SyscallReturn {
        syscall_name: Option<String>,
    },
    /// Any reason the GDB manual does not document, carried verbatim.
    /// This is the forward-compat escape hatch.
    Other(String),
}

impl StoppedReason {
    /// Parse a `*stopped` record's results into the typed variant.
    ///
    /// Extracts the `reason` field plus any variant-specific payload fields
    /// (`bkptno`, `exit-code`, `signal-name`, `syscall-name`). Returns
    /// `None` when the `reason` field is absent.
    #[must_use]
    pub fn from_results(results: &[(String, Value)]) -> Option<Self> {
        let name = get_str(results, "reason")?;
        Some(match name {
            "breakpoint-hit" => Self::BreakpointHit {
                bkptno: get_string(results, "bkptno"),
            },
            "watchpoint-trigger" => Self::WatchpointTrigger,
            "function-finished" => Self::FunctionFinished,
            "location-reached" => Self::LocationReached,
            "end-stepping-range" => Self::EndSteppingRange,
            "exited-normally" => Self::ExitedNormally,
            "exited" => Self::Exited {
                exit_code: get_i32(results, "exit-code"),
            },
            "exited-signalled" => Self::ExitedSignalled {
                signal_name: get_string(results, "signal-name"),
            },
            "signal-received" => Self::SignalReceived {
                signal_name: get_string(results, "signal-name"),
            },
            "fork" => Self::Fork,
            "vfork" => Self::Vfork,
            "exec" => Self::Exec,
            "syscall-entry" => Self::SyscallEntry {
                syscall_name: get_string(results, "syscall-name"),
            },
            "syscall-return" => Self::SyscallReturn {
                syscall_name: get_string(results, "syscall-name"),
            },
            other => Self::Other(other.to_string()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use framewalk_mi_codec::Value;

    fn results(pairs: &[(&str, &str)]) -> Vec<(String, Value)> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), Value::Const((*v).to_string())))
            .collect()
    }

    // ---- TargetState predicates ----

    #[test]
    fn default_is_unknown() {
        let s = TargetState::default();
        assert!(!s.is_running());
        assert!(!s.is_stopped());
        assert!(!s.is_exited());
    }

    #[test]
    fn running_predicate() {
        let s = TargetState::Running {
            thread: Some(ThreadId::new("1")),
        };
        assert!(s.is_running());
        assert!(!s.is_stopped());
        assert!(!s.is_exited());
    }

    #[test]
    fn stopped_predicate() {
        let s = TargetState::Stopped {
            thread: None,
            reason: None,
        };
        assert!(!s.is_running());
        assert!(s.is_stopped());
        assert!(!s.is_exited());
    }

    #[test]
    fn exited_predicate() {
        let s = TargetState::Exited { exit_code: Some(0) };
        assert!(!s.is_running());
        assert!(!s.is_stopped());
        assert!(s.is_exited());
    }

    #[test]
    fn mark_unknown_resets_any_state() {
        let mut s = TargetState::Running { thread: None };
        s.mark_unknown();
        assert_eq!(s, TargetState::Unknown);

        let mut s = TargetState::Exited {
            exit_code: Some(137),
        };
        s.mark_unknown();
        assert_eq!(s, TargetState::Unknown);
    }

    // ---- StoppedReason::from_results ----

    #[test]
    fn stopped_reason_missing_returns_none() {
        assert!(StoppedReason::from_results(&results(&[])).is_none());
        assert!(StoppedReason::from_results(&results(&[("other", "x")])).is_none());
    }

    #[test]
    fn stopped_reason_breakpoint_hit_carries_bkptno() {
        let r =
            StoppedReason::from_results(&results(&[("reason", "breakpoint-hit"), ("bkptno", "7")]));
        assert_eq!(
            r,
            Some(StoppedReason::BreakpointHit {
                bkptno: Some("7".into()),
            })
        );
    }

    #[test]
    fn stopped_reason_breakpoint_hit_without_bkptno() {
        let r = StoppedReason::from_results(&results(&[("reason", "breakpoint-hit")]));
        assert_eq!(r, Some(StoppedReason::BreakpointHit { bkptno: None }));
    }

    #[test]
    fn stopped_reason_unit_variants() {
        let cases = [
            ("watchpoint-trigger", StoppedReason::WatchpointTrigger),
            ("function-finished", StoppedReason::FunctionFinished),
            ("location-reached", StoppedReason::LocationReached),
            ("end-stepping-range", StoppedReason::EndSteppingRange),
            ("exited-normally", StoppedReason::ExitedNormally),
            ("fork", StoppedReason::Fork),
            ("vfork", StoppedReason::Vfork),
            ("exec", StoppedReason::Exec),
        ];
        for (name, expected) in cases {
            let got = StoppedReason::from_results(&results(&[("reason", name)]));
            assert_eq!(got, Some(expected), "reason={name}");
        }
    }

    #[test]
    fn stopped_reason_exited_with_code() {
        let r = StoppedReason::from_results(&results(&[("reason", "exited"), ("exit-code", "42")]));
        assert_eq!(
            r,
            Some(StoppedReason::Exited {
                exit_code: Some(42)
            })
        );
    }

    #[test]
    fn stopped_reason_exited_without_code() {
        let r = StoppedReason::from_results(&results(&[("reason", "exited")]));
        assert_eq!(r, Some(StoppedReason::Exited { exit_code: None }));
    }

    #[test]
    fn stopped_reason_exited_signalled_carries_name() {
        let r = StoppedReason::from_results(&results(&[
            ("reason", "exited-signalled"),
            ("signal-name", "SIGSEGV"),
        ]));
        assert_eq!(
            r,
            Some(StoppedReason::ExitedSignalled {
                signal_name: Some("SIGSEGV".into()),
            })
        );
    }

    #[test]
    fn stopped_reason_signal_received_carries_name() {
        let r = StoppedReason::from_results(&results(&[
            ("reason", "signal-received"),
            ("signal-name", "SIGINT"),
        ]));
        assert_eq!(
            r,
            Some(StoppedReason::SignalReceived {
                signal_name: Some("SIGINT".into()),
            })
        );
    }

    #[test]
    fn stopped_reason_syscall_entry_and_return() {
        let entry = StoppedReason::from_results(&results(&[
            ("reason", "syscall-entry"),
            ("syscall-name", "openat"),
        ]));
        assert_eq!(
            entry,
            Some(StoppedReason::SyscallEntry {
                syscall_name: Some("openat".into()),
            })
        );
        let ret = StoppedReason::from_results(&results(&[
            ("reason", "syscall-return"),
            ("syscall-name", "read"),
        ]));
        assert_eq!(
            ret,
            Some(StoppedReason::SyscallReturn {
                syscall_name: Some("read".into()),
            })
        );
    }

    #[test]
    fn stopped_reason_unknown_preserved_as_other() {
        // Forward-compat escape hatch: any reason the manual hasn't
        // enumerated must pass through verbatim instead of being dropped.
        let r = StoppedReason::from_results(&results(&[("reason", "framewalk-invented")]));
        assert_eq!(r, Some(StoppedReason::Other("framewalk-invented".into())));
    }
}
