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
/// `target_state`, `drain_events`, `target_select`, `reconnect_target`).
#[derive(Debug, Clone, Copy, Default, clap::ValueEnum)]
pub enum Mode {
    /// All semantic GDB tools plus `scheme_eval`.
    #[value(alias = "standard")]
    Full,
    /// Curated MI-first subset plus the raw and Scheme escape hatches.
    Core,
    /// Only `scheme_eval` — minimal context-window footprint.
    #[default]
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

    /// Operating mode. `scheme` exposes `scheme_eval` plus the operator
    /// escape hatches; `full` exposes the complete MI-first surface;
    /// `core` exposes the common subset plus `mi_raw_command` and
    /// `scheme_eval`. The legacy value `standard` is accepted as an
    /// alias for `full`.
    #[arg(long, env = "FRAMEWALK_MODE", default_value = "scheme")]
    pub mode: Mode,

    /// Connect to a remote target during startup, before the first tool
    /// call. Format is `<transport>:<parameters>`, e.g.
    /// `remote:localhost:1234` or `extended-remote:host.example:9999`;
    /// only the first colon separates the two halves, so `host:port`
    /// parameters survive intact.
    ///
    /// Equivalent to calling the `target_select` tool immediately after
    /// startup — same non-stop downgrade retry, same vmlinux probe — but
    /// it means `reconnect_target` has a valid selection from the very
    /// first tool call. A failure here is fatal: booting into a session
    /// that silently did not connect is worse than exiting.
    #[arg(long, env = "FRAMEWALK_CONNECT", value_name = "TRANSPORT:PARAMS")]
    pub connect: Option<String>,

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
