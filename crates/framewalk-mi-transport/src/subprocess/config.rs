//! Configuration for spawning a GDB subprocess.
//!
//! [`GdbConfig`] is the one place where framewalk's launch defaults for
//! GDB live. The critical defaults are `--quiet` (suppresses the banner,
//! keeping first-bytes handling clean) and `--nx` (skips user `.gdbinit`,
//! which otherwise injects arbitrary commands and breaks reproducibility).
//! Neither can be turned off via the config API on purpose — if a caller
//! legitimately needs `.gdbinit` behaviour they can fork this crate.

use std::path::PathBuf;

use framewalk_mi_protocol::MiVersion;

/// Configuration for [`crate::subprocess::spawn`].
#[derive(Debug, Clone)]
pub struct GdbConfig {
    /// Path to the `gdb` binary. Defaults to `"gdb"` (resolved via `PATH`).
    pub program: String,
    /// Which GDB/MI interpreter version to request. Defaults to
    /// [`MiVersion::Mi3`]; `Unknown` also maps to mi3 via
    /// [`MiVersion::as_interpreter_arg`].
    pub mi_version: MiVersion,
    /// Extra arguments passed to GDB after the mandatory
    /// `--interpreter=miN --quiet --nx` flags.
    pub extra_args: Vec<String>,
    /// Environment variables to set for the GDB child. Merged with the
    /// inherited environment; entries here override the parent.
    pub env: Vec<(String, String)>,
    /// Working directory for the GDB child. `None` inherits the parent's.
    pub cwd: Option<PathBuf>,
    /// Enable GDB non-stop mode (`-gdb-set non-stop on`) during bootstrap.
    /// Defaults to `true`.  Set to `false` for remote stubs that only speak
    /// all-stop (e.g. QEMU's gdbstub, many bare-metal JTAG probes).
    pub non_stop: bool,
}

impl Default for GdbConfig {
    fn default() -> Self {
        Self {
            program: "gdb".to_string(),
            mi_version: MiVersion::Mi3,
            extra_args: Vec::new(),
            env: Vec::new(),
            cwd: None,
            non_stop: true,
        }
    }
}

impl GdbConfig {
    /// A config with all defaults: `gdb` from `PATH`, mi3 interpreter,
    /// no extra args, inherited env and cwd.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Override the path to the GDB binary.
    #[must_use]
    pub fn with_program(mut self, program: impl Into<String>) -> Self {
        self.program = program.into();
        self
    }

    /// Override the MI version.
    #[must_use]
    pub fn with_mi_version(mut self, version: MiVersion) -> Self {
        self.mi_version = version;
        self
    }

    /// Append an extra command-line argument.
    #[must_use]
    pub fn with_arg(mut self, arg: impl Into<String>) -> Self {
        self.extra_args.push(arg.into());
        self
    }

    /// Set an environment variable for the child.
    #[must_use]
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    /// Override the child's working directory.
    #[must_use]
    pub fn with_cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    /// Enable or disable GDB non-stop mode.  Defaults to `true`.
    #[must_use]
    pub fn with_non_stop(mut self, enabled: bool) -> Self {
        self.non_stop = enabled;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Defaults ----

    #[test]
    fn default_targets_gdb_on_path_with_mi3_and_non_stop() {
        let c = GdbConfig::default();
        assert_eq!(c.program, "gdb");
        assert_eq!(c.mi_version, MiVersion::Mi3);
        assert!(c.extra_args.is_empty());
        assert!(c.env.is_empty());
        assert!(c.cwd.is_none());
        assert!(c.non_stop);
    }

    #[test]
    fn new_is_default() {
        let a = GdbConfig::new();
        let b = GdbConfig::default();
        assert_eq!(a.program, b.program);
        assert_eq!(a.mi_version, b.mi_version);
        assert_eq!(a.non_stop, b.non_stop);
    }

    // ---- Individual builders ----

    #[test]
    fn with_program_overrides_binary_path() {
        let c = GdbConfig::new().with_program("/usr/local/bin/gdb");
        assert_eq!(c.program, "/usr/local/bin/gdb");
    }

    #[test]
    fn with_mi_version_overrides_interpreter() {
        let c = GdbConfig::new().with_mi_version(MiVersion::Mi2);
        assert_eq!(c.mi_version, MiVersion::Mi2);
    }

    #[test]
    fn with_non_stop_toggles_flag() {
        let c = GdbConfig::new().with_non_stop(false);
        assert!(!c.non_stop);
        let c = c.with_non_stop(true);
        assert!(c.non_stop);
    }

    #[test]
    fn with_cwd_stores_path_buf() {
        use std::path::PathBuf;
        let c = GdbConfig::new().with_cwd("/tmp/work");
        assert_eq!(c.cwd, Some(PathBuf::from("/tmp/work")));
    }

    // ---- Accumulating builders ----

    #[test]
    fn with_arg_appends_and_preserves_order() {
        let c = GdbConfig::new().with_arg("--batch").with_arg("--ex=quit");
        assert_eq!(c.extra_args, vec!["--batch", "--ex=quit"]);
    }

    #[test]
    fn with_env_appends_in_order_allowing_duplicates() {
        let c = GdbConfig::new()
            .with_env("PATH", "/usr/bin")
            .with_env("LD_LIBRARY_PATH", "/lib")
            .with_env("PATH", "/usr/local/bin");
        // Duplicates intentionally preserved — the spawn layer merges with
        // the inherited environment and "last write wins" there.
        assert_eq!(c.env.len(), 3);
        assert_eq!(c.env[0], ("PATH".to_string(), "/usr/bin".to_string()));
        assert_eq!(c.env[2], ("PATH".to_string(), "/usr/local/bin".to_string()));
    }

    // ---- Full chain composition ----

    #[test]
    fn builder_chain_produces_expected_aggregate_state() {
        let c = GdbConfig::new()
            .with_program("/opt/gdb")
            .with_mi_version(MiVersion::Mi3)
            .with_arg("--nh")
            .with_env("HOME", "/tmp")
            .with_cwd("/tmp/project")
            .with_non_stop(false);
        assert_eq!(c.program, "/opt/gdb");
        assert_eq!(c.mi_version, MiVersion::Mi3);
        assert_eq!(c.extra_args, vec!["--nh"]);
        assert_eq!(c.env, vec![("HOME".to_string(), "/tmp".to_string())]);
        assert_eq!(c.cwd.as_deref(), Some(std::path::Path::new("/tmp/project")));
        assert!(!c.non_stop);
    }
}
