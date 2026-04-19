use super::prelude::*;
use super::{FULL_CORE, FULL_ONLY};

framewalk_tool_block! {
    router: context_core_tool_router,
    specs: CONTEXT_CORE_TOOL_SPECS,
    category: "context",
    profiles: FULL_CORE,
    names: [set_args, set_cwd, show_cwd];
    items: {
        #[tool(description = "Set program arguments for the next `-exec-run`.")]
        async fn set_args(
            &self,
            Parameters(args): Parameters<context::SetArgsArgs>,
        ) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("exec-arguments").parameter(args.args))
                .await
        }

        #[tool(description = "Change GDB's working directory.")]
        async fn set_cwd(
            &self,
            Parameters(args): Parameters<context::SetCwdArgs>,
        ) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("environment-cd").parameter(args.directory))
                .await
        }

        #[tool(description = "Show GDB's current working directory.")]
        async fn show_cwd(&self) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("environment-pwd"))
                .await
        }
    }
}

framewalk_tool_block! {
    router: context_extended_tool_router,
    specs: CONTEXT_EXTENDED_TOOL_SPECS,
    category: "context",
    profiles: FULL_ONLY,
    names: [set_inferior_tty, show_inferior_tty, environment_directory, environment_path];
    items: {
        #[tool(description = "Set the inferior's terminal device (TTY).")]
        async fn set_inferior_tty(
            &self,
            Parameters(args): Parameters<context::SetInferiorTtyArgs>,
        ) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("inferior-tty-set").parameter(args.tty))
                .await
        }

        #[tool(description = "Show the inferior's terminal device.")]
        async fn show_inferior_tty(&self) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("inferior-tty-show"))
                .await
        }

        #[tool(description = "Add directories to the source file search path.")]
        async fn environment_directory(
            &self,
            Parameters(args): Parameters<context::EnvironmentDirectoryArgs>,
        ) -> Result<CallToolResult, McpError> {
            let mut cmd = MiCommand::new("environment-directory");
            if args.reset {
                cmd = cmd.option("r");
            }
            for d in &args.directories {
                cmd = cmd.parameter(d.clone());
            }
            self.submit_as_tool_result(cmd).await
        }

        #[tool(description = "Set the executable search path (PATH for finding programs).")]
        async fn environment_path(
            &self,
            Parameters(args): Parameters<context::EnvironmentPathArgs>,
        ) -> Result<CallToolResult, McpError> {
            let mut cmd = MiCommand::new("environment-path");
            if args.reset {
                cmd = cmd.option("r");
            }
            for d in &args.directories {
                cmd = cmd.parameter(d.clone());
            }
            self.submit_as_tool_result(cmd).await
        }
    }
}

framewalk_tool_block! {
    router: file_tool_router,
    specs: FILE_TOOL_SPECS,
    category: "file",
    profiles: FULL_ONLY,
    names: [exec_file, symbol_file, list_source_files, list_shared_libraries, list_exec_source_file];
    items: {
        #[tool(description = "Rarely what you want — use `load_file` unless you \
                              intentionally need to set the executable without \
                              loading symbols.")]
        async fn exec_file(
            &self,
            Parameters(args): Parameters<file::FilePathArgs>,
        ) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("file-exec-file").parameter(args.path))
                .await
        }

        #[tool(description = "Load a symbol file separately from the executable.")]
        async fn symbol_file(
            &self,
            Parameters(args): Parameters<file::FilePathArgs>,
        ) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("file-symbol-file").parameter(args.path))
                .await
        }

        #[tool(description = "List source files known to GDB.")]
        async fn list_source_files(
            &self,
            Parameters(args): Parameters<file::ListSourceFilesArgs>,
        ) -> Result<CallToolResult, McpError> {
            let mut cmd = MiCommand::new("file-list-exec-source-files");
            if args.group_by_objfile {
                cmd = cmd.option("group-by-objfile");
            }
            if let Some(re) = args.regexp {
                cmd = cmd.parameter(re);
            }
            self.submit_as_tool_result(cmd).await
        }

        #[tool(description = "List shared libraries loaded by the target.")]
        async fn list_shared_libraries(
            &self,
            Parameters(args): Parameters<file::ListSharedLibrariesArgs>,
        ) -> Result<CallToolResult, McpError> {
            let mut cmd = MiCommand::new("file-list-shared-libraries");
            if let Some(re) = args.regexp {
                cmd = cmd.parameter(re);
            }
            self.submit_as_tool_result(cmd).await
        }

        #[tool(description = "Show info about the currently executing source file \
                           (line, file, fullname, macro-info).")]
        async fn list_exec_source_file(&self) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("file-list-exec-source-file"))
                .await
        }
    }
}

framewalk_tool_block! {
    router: target_tool_router,
    specs: TARGET_TOOL_SPECS,
    category: "target",
    profiles: FULL_ONLY,
    names: [target_select, target_download, target_disconnect, target_flash_erase];
    items: {
        #[tool(description = "Connect to a remote target (e.g. gdbserver). \
                              If the stub rejects non-stop mode, framewalk \
                              automatically disables non-stop and retries once; \
                              the downgrade is surfaced as a `warning:` entry \
                              visible via `drain-events`.")]
        async fn target_select(
            &self,
            Parameters(args): Parameters<target::TargetSelectArgs>,
        ) -> Result<CallToolResult, McpError> {
            let transport_name = args.transport;
            let parameters = args.parameters;
            let rebuild = || {
                MiCommand::new("target-select")
                    .parameter(transport_name.clone())
                    .parameter(parameters.clone())
            };

            let initial = self.submit_command(rebuild()).await?;
            let outcome = match initial {
                framewalk_mi_protocol::CommandOutcome::Error { ref msg, .. }
                    if crate::server_helpers::is_non_stop_mismatch(msg) =>
                {
                    crate::server_helpers::downgrade_non_stop_and_retry(
                        self.transport_handle(),
                        msg,
                        &rebuild,
                    )
                    .await?
                }
                other => other,
            };

            crate::server_helpers::remember_successful_target_select_command(
                self.transport_handle(),
                &rebuild(),
                &outcome,
            );

            // Best-effort vmlinux detection, gated to remote-class
            // transports so local/native connects pay no extra cost.
            if matches!(
                outcome,
                framewalk_mi_protocol::CommandOutcome::Done(_)
                    | framewalk_mi_protocol::CommandOutcome::Connected(_)
            ) && crate::server_helpers::is_remote_target_transport(&transport_name)
            {
                crate::server_helpers::spawn_vmlinux_probe(self.transport_arc());
            }

            Ok(crate::server_helpers::format_outcome(&outcome))
        }

        #[tool(description = "Download the executable to the remote target.")]
        async fn target_download(&self) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("target-download"))
                .await
        }

        #[tool(description = "Disconnect from the remote target.")]
        async fn target_disconnect(&self) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("target-disconnect"))
                .await
        }

        #[tool(description = "Erase all known flash memory regions on the target.")]
        async fn target_flash_erase(&self) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("target-flash-erase"))
                .await
        }
    }
}

framewalk_tool_block! {
    router: file_transfer_tool_router,
    specs: FILE_TRANSFER_TOOL_SPECS,
    category: "file-transfer",
    profiles: FULL_ONLY,
    names: [target_file_put, target_file_get, target_file_delete];
    items: {
        #[tool(description = "Copy a file from the host to the remote target.")]
        async fn target_file_put(
            &self,
            Parameters(args): Parameters<file_transfer::FilePutArgs>,
        ) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(
                MiCommand::new("target-file-put")
                    .parameter(args.host_file)
                    .parameter(args.target_file),
            )
            .await
        }

        #[tool(description = "Copy a file from the remote target to the host.")]
        async fn target_file_get(
            &self,
            Parameters(args): Parameters<file_transfer::FileGetArgs>,
        ) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(
                MiCommand::new("target-file-get")
                    .parameter(args.target_file)
                    .parameter(args.host_file),
            )
            .await
        }

        #[tool(description = "Delete a file on the remote target.")]
        async fn target_file_delete(
            &self,
            Parameters(args): Parameters<file_transfer::FileDeleteArgs>,
        ) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("target-file-delete").parameter(args.target_file))
                .await
        }
    }
}

framewalk_tool_block! {
    router: support_core_tool_router,
    specs: SUPPORT_CORE_TOOL_SPECS,
    category: "support",
    profiles: FULL_CORE,
    names: [list_features, list_target_features];
    items: {
        #[tool(description = "List GDB/MI interpreter features.")]
        async fn list_features(&self) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("list-features"))
                .await
        }

        #[tool(description = "List target-specific features (e.g. async, reverse).")]
        async fn list_target_features(&self) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("list-target-features"))
                .await
        }
    }
}

framewalk_tool_block! {
    router: support_extended_tool_router,
    specs: SUPPORT_EXTENDED_TOOL_SPECS,
    category: "support",
    profiles: FULL_ONLY,
    names: [info_mi_command, gdb_set, gdb_show];
    items: {
        #[tool(description = "Query whether a specific MI command exists.")]
        async fn info_mi_command(
            &self,
            Parameters(args): Parameters<support::InfoMiCommandArgs>,
        ) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("info-gdb-mi-command").parameter(args.command))
                .await
        }

        #[tool(description = "Set a GDB variable (e.g. `pagination off`).")]
        async fn gdb_set(
            &self,
            Parameters(args): Parameters<support::GdbSetArgs>,
        ) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("gdb-set").parameter(args.variable))
                .await
        }

        #[tool(description = "Show a GDB variable's current value.")]
        async fn gdb_show(
            &self,
            Parameters(args): Parameters<support::GdbShowArgs>,
        ) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("gdb-show").parameter(args.variable))
                .await
        }
    }
}

framewalk_tool_block! {
    router: misc_tool_router,
    specs: MISC_TOOL_SPECS,
    category: "misc",
    profiles: FULL_ONLY,
    names: [
        list_thread_groups,
        info_os,
        add_inferior,
        remove_inferior,
        thread_list_ids,
        ada_task_info,
        info_ada_exceptions,
        enable_timings,
        complete,
    ];
    items: {
        #[tool(description = "List thread groups (inferiors) on the target.")]
        async fn list_thread_groups(
            &self,
            Parameters(args): Parameters<misc::ListThreadGroupsArgs>,
        ) -> Result<CallToolResult, McpError> {
            let mut cmd = MiCommand::new("list-thread-groups");
            if args.available {
                cmd = cmd.option("available");
            }
            if args.recurse {
                cmd = cmd.option_with("recurse", "1");
            }
            for g in &args.groups {
                cmd = cmd.parameter(g.clone());
            }
            self.submit_as_tool_result(cmd).await
        }

        #[tool(description = "Query OS-level information (processes, threads, etc).")]
        async fn info_os(
            &self,
            Parameters(args): Parameters<misc::InfoOsArgs>,
        ) -> Result<CallToolResult, McpError> {
            let mut cmd = MiCommand::new("info-os");
            if let Some(t) = args.info_type {
                cmd = cmd.parameter(t);
            }
            self.submit_as_tool_result(cmd).await
        }

        #[tool(description = "Add a new inferior (debuggee process slot).")]
        async fn add_inferior(&self) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("add-inferior"))
                .await
        }

        #[tool(description = "Remove an exited inferior by id.")]
        async fn remove_inferior(
            &self,
            Parameters(args): Parameters<misc::RemoveInferiorArgs>,
        ) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("remove-inferior").parameter(args.inferior_id))
                .await
        }

        #[tool(description = "List thread ids in the target.")]
        async fn thread_list_ids(&self) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("thread-list-ids"))
                .await
        }

        #[tool(description = "Query Ada tasks (requires Ada program).")]
        async fn ada_task_info(&self) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("ada-task-info"))
                .await
        }

        #[tool(description = "List defined Ada exceptions (optionally filtered by regexp).")]
        async fn info_ada_exceptions(
            &self,
            Parameters(args): Parameters<misc::InfoAdaExceptionsArgs>,
        ) -> Result<CallToolResult, McpError> {
            let mut cmd = MiCommand::new("info-ada-exceptions");
            if let Some(re) = args.regexp {
                cmd = cmd.parameter(re);
            }
            self.submit_as_tool_result(cmd).await
        }

        #[tool(description = "Enable or disable collection of timing statistics for MI commands.")]
        async fn enable_timings(
            &self,
            Parameters(args): Parameters<misc::EnableTimingsArgs>,
        ) -> Result<CallToolResult, McpError> {
            let flag = if args.enable { "yes" } else { "no" };
            self.submit_as_tool_result(MiCommand::new("enable-timings").parameter(flag))
                .await
        }

        #[tool(description = "Return possible completions for a partial GDB CLI command.")]
        async fn complete(
            &self,
            Parameters(args): Parameters<misc::CompleteArgs>,
        ) -> Result<CallToolResult, McpError> {
            self.submit_as_tool_result(MiCommand::new("complete").parameter(args.command))
                .await
        }
    }
}
