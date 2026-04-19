//! Command-line configuration for `framewalk-mcp`.
//!
//! Parsed once at startup and threaded into the server. Every flag maps
//! to a real operational concern — this is the knob surface operators
//! will actually touch when wiring framewalk into their MCP client.

use std::path::PathBuf;

use clap::Parser;

/// Operating mode. Controls which tools are exposed to the MCP client.
///
/// `Full` registers the complete semantic GDB/MI tool surface **plus**
/// `scheme_eval`.
///
/// `Core` keeps framewalk MI-first, but exposes only the common
/// day-to-day debugger operations plus the `mi_raw_command` and
/// `scheme_eval` escape hatches.
///
/// `Scheme` keeps the tool-definition payload minimal by registering
/// `scheme_eval` plus a small operator surface (`interrupt_target`,
/// `target_state`, `drain_events`, `reconnect_target`).
#[derive(Debug, Clone, Copy, Default, clap::ValueEnum)]
pub enum Mode {
    /// All semantic GDB tools plus `scheme_eval`.
    #[default]
    #[value(alias = "standard")]
    Full,
    /// Curated MI-first subset plus the raw and Scheme escape hatches.
    Core,
    /// Only `scheme_eval` — minimal context-window footprint.
    Scheme,
}

/// MCP server exposing GDB/MI debugging as tools and live resources.
#[derive(Debug, Clone, Parser)]
#[command(name = "framewalk-mcp", version, about)]
pub struct Config {
    /// Path to the `gdb` binary to spawn. Defaults to `gdb` resolved via
    /// `PATH`.
    #[arg(long, env = "FRAMEWALK_GDB", default_value = "gdb")]
    pub gdb: String,

    /// Working directory for the spawned GDB child. Defaults to the
    /// MCP server's own working directory.
    #[arg(long)]
    pub cwd: Option<PathBuf>,

    /// Operating mode. `full` exposes the complete MI-first surface;
    /// `core` exposes the common subset plus `mi_raw_command` and
    /// `scheme_eval`; `scheme` exposes `scheme_eval` plus the operator
    /// escape hatches. The legacy value `standard` is accepted as an
    /// alias for `full`.
    #[arg(long, env = "FRAMEWALK_MODE", default_value = "full")]
    pub mode: Mode,

    /// Enable GDB non-stop mode during session bootstrap.  Defaults to
    /// `true`.  Pass `--no-non-stop` when connecting to remote stubs
    /// that only speak all-stop (e.g. QEMU's gdbstub, many JTAG probes).
    #[arg(long, env = "FRAMEWALK_NON_STOP", default_value_t = true)]
    pub non_stop: bool,

    /// **Security boundary.** Allow the `mi_raw_command` tool to pass
    /// MI commands that invoke shell escapes — `-interpreter-exec
    /// console`, `shell ...`, `!...`, and related. Default-deny because
    /// letting an LLM run arbitrary shell inside the debugger host is
    /// a serious capability. Only enable in contained environments.
    #[arg(long, env = "FRAMEWALK_ALLOW_SHELL", default_value_t = false)]
    pub allow_shell: bool,

    /// `tracing_subscriber` log filter (e.g. `framewalk=debug,rmcp=info`).
    /// Logs always go to stderr — stdout is reserved for MCP protocol
    /// traffic on the stdio transport.
    #[arg(
        long,
        env = "FRAMEWALK_LOG",
        default_value = "framewalk=info,rmcp=warn"
    )]
    pub log: String,

    /// Default timeout for a single `scheme_eval` call, in seconds.
    #[arg(long, env = "FRAMEWALK_SCHEME_EVAL_TIMEOUT_SECS", default_value_t = 60)]
    pub scheme_eval_timeout_secs: u64,

    /// Default timeout for Scheme stop waits (`wait-for-stop`,
    /// `run-and-wait`, `cont-and-wait`, etc.), in seconds.
    #[arg(
        long,
        env = "FRAMEWALK_WAIT_FOR_STOP_TIMEOUT_SECS",
        default_value_t = 30
    )]
    pub wait_for_stop_timeout_secs: u64,
}
