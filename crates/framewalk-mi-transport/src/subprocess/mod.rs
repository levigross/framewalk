//! The [`spawn`] entry point: launch GDB as a subprocess and return a
//! [`TransportHandle`](crate::handle::TransportHandle) bound to its stdio.

pub(crate) mod config;

use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use framewalk_mi_protocol::{CommandOutcome, Connection};
use tokio::process::Command;
use tokio::sync::mpsc;
use tracing::{debug, info};

use crate::error::TransportError;
use crate::handle::TransportHandle;
use crate::pump::{run_reader, run_stderr_logger, run_writer};
use crate::shared::SharedState;

pub use config::GdbConfig;

/// Capacity of the outbound byte-buffer channel between [`TransportHandle::submit`]
/// and the writer task. 64 pending command buffers is plenty for any
/// realistic frontend; each buffer holds one complete encoded command.
const WRITE_CHANNEL_CAPACITY: usize = 64;

/// Session bootstrap commands that must succeed before framewalk serves
/// requests. These shape the MI semantics the rest of the stack relies on:
/// async command completion and no interactive pager or confirmation prompts.
/// Non-stop mode is conditional — see [`GdbConfig::non_stop`].
const BOOTSTRAP_COMMANDS: &[&str] = &[
    "-gdb-set mi-async on",
    "-gdb-set pagination off",
    "-gdb-set confirm off",
];

/// Spawn a GDB subprocess with the given [`GdbConfig`] and return an
/// async [`TransportHandle`] ready to submit commands.
///
/// The child process is launched with `kill_on_drop` enabled so it
/// cannot outlive the handle in an error path. Its stdio is piped into
/// three background tokio tasks (reader, writer, stderr logger) that
/// handle the full byte pipeline. The caller only interacts with the
/// returned handle.
///
/// # Errors
///
/// Returns [`TransportError::Spawn`] if the child cannot be started
/// (e.g. `gdb` not on `PATH`) or [`TransportError::PipeMissing`] if
/// tokio fails to attach a stdio pipe for some reason.
pub async fn spawn(config: GdbConfig) -> Result<TransportHandle, TransportError> {
    info!(
        program = %config.program,
        mi_version = ?config.mi_version,
        extra_args = ?config.extra_args,
        "spawning gdb subprocess"
    );

    let mut cmd = Command::new(&config.program);
    cmd.arg(format!(
        "--interpreter={}",
        config.mi_version.as_interpreter_arg()
    ))
    .arg("--quiet")
    .arg("--nx");

    for arg in &config.extra_args {
        cmd.arg(arg);
    }
    for (k, v) in &config.env {
        cmd.env(k, v);
    }
    if let Some(cwd) = &config.cwd {
        cmd.current_dir(cwd);
    }

    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = cmd.spawn().map_err(TransportError::Spawn)?;
    let stdin = child
        .stdin
        .take()
        .ok_or(TransportError::PipeMissing("stdin"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or(TransportError::PipeMissing("stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or(TransportError::PipeMissing("stderr"))?;

    debug!(pid = ?child.id(), "gdb spawned");

    // Build shared state and the write channel.
    let shared = Arc::new(SharedState::new(Connection::new()));
    let (write_tx, write_rx) = mpsc::channel::<Vec<u8>>(WRITE_CHANNEL_CAPACITY);

    // Spawn the three background tasks and retain their JoinHandles so
    // `TransportHandle::shutdown` can observe task termination
    // deterministically instead of leaving them detached.
    let reader_task = tokio::spawn({
        let shared = Arc::clone(&shared);
        async move {
            if let Err(err) = run_reader(shared, stdout).await {
                tracing::error!(%err, "reader task exited with error");
            }
        }
    });
    let writer_task = tokio::spawn(async move {
        if let Err(err) = run_writer(stdin, write_rx).await {
            tracing::error!(%err, "writer task exited with error");
        }
    });
    let stderr_task = tokio::spawn(async move {
        if let Err(err) = run_stderr_logger(stderr).await {
            tracing::error!(%err, "stderr logger task exited with error");
        }
    });

    let handle = TransportHandle {
        shared,
        write_tx,
        child,
        reader_task,
        writer_task,
        stderr_task,
    };

    bootstrap_session(&handle, config.non_stop).await?;

    Ok(handle)
}

async fn bootstrap_session(handle: &TransportHandle, non_stop: bool) -> Result<(), TransportError> {
    let non_stop_cmd = "-gdb-set non-stop on";
    let commands: Vec<&str> = if non_stop {
        let mut cmds: Vec<&str> = BOOTSTRAP_COMMANDS.to_vec();
        cmds.insert(1, non_stop_cmd);
        cmds
    } else {
        BOOTSTRAP_COMMANDS.to_vec()
    };

    for raw in &commands {
        let Ok(outcome) =
            tokio::time::timeout(Duration::from_secs(2), handle.submit_raw(raw)).await
        else {
            return Err(TransportError::Bootstrap {
                command: (*raw).to_string(),
                message: "timed out waiting for bootstrap reply".to_string(),
            });
        };

        match outcome {
            Ok(CommandOutcome::Done(_) | CommandOutcome::Connected(_)) => {}
            Ok(CommandOutcome::Error { msg, .. }) => {
                return Err(TransportError::Bootstrap {
                    command: (*raw).to_string(),
                    message: msg,
                });
            }
            Ok(other) => {
                return Err(TransportError::Bootstrap {
                    command: (*raw).to_string(),
                    message: format!("unexpected outcome: {other:?}"),
                });
            }
            Err(err) => return Err(err),
        }
    }

    Ok(())
}
