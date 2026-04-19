//! Step 4 tests: drive a `Connection` through realistic multi-event
//! sessions and assert the state registries (target, threads, frames,
//! breakpoints, varobjs, features) update correctly.

use framewalk_mi_codec::MiCommand;
use framewalk_mi_protocol::{
    BreakpointId, CommandRequest, Connection, StoppedReason, TargetState, ThreadGroupId, ThreadId,
    ThreadState, VarObjName,
};

/// Drain and discard every currently-queued event. Used when the events
/// themselves aren't the focus — we're asserting against the state
/// registries after the dust settles.
fn drain(conn: &mut Connection) {
    while conn.poll_event().is_some() {}
}

/// Submit a command and advance the outbound cursor past its bytes, so
/// subsequent `outbound()` calls start fresh.
fn submit_and_ack(conn: &mut Connection, cmd: MiCommand) -> framewalk_mi_protocol::CommandHandle {
    let handle = conn.submit(CommandRequest::new(cmd));
    let n = conn.outbound().len();
    conn.consume_outbound(n).unwrap();
    handle
}

// ---------------------------------------------------------------------------
// TargetState transitions
// ---------------------------------------------------------------------------

#[test]
fn target_state_starts_unknown() {
    let conn = Connection::new();
    assert_eq!(*conn.target_state(), TargetState::Unknown);
}

#[test]
fn running_transitions_to_running_state() {
    let mut conn = Connection::new();
    conn.receive_bytes(b"*running,thread-id=\"all\"\n(gdb)\n")
        .unwrap();
    drain(&mut conn);
    match conn.target_state() {
        TargetState::Running { thread: Some(t) } => assert!(t.is_all()),
        other => panic!("expected Running(all), got {other:?}"),
    }
}

#[test]
fn stopped_transitions_to_stopped_state_with_reason() {
    let mut conn = Connection::new();
    conn.receive_bytes(
        b"*stopped,reason=\"breakpoint-hit\",disp=\"keep\",bkptno=\"1\",thread-id=\"1\",\
          frame={level=\"0\",addr=\"0x400500\",func=\"main\",file=\"hello.c\",\
          fullname=\"/tmp/hello.c\",line=\"3\"}\n(gdb)\n",
    )
    .unwrap();
    drain(&mut conn);
    match conn.target_state() {
        TargetState::Stopped { thread, reason } => {
            assert_eq!(thread.as_ref().map(ThreadId::as_str), Some("1"));
            assert!(matches!(reason, Some(StoppedReason::BreakpointHit { .. })));
        }
        other => panic!("expected Stopped, got {other:?}"),
    }
}

#[test]
fn error_on_exec_command_resets_target_to_unknown_and_clears_frames() {
    let mut conn = Connection::new();

    // Force the target into a known-Stopped state first with frame info.
    conn.receive_bytes(
        b"*stopped,reason=\"breakpoint-hit\",thread-id=\"1\",\
          frame={level=\"0\",func=\"main\",file=\"hello.c\",line=\"3\"}\n(gdb)\n",
    )
    .unwrap();
    drain(&mut conn);
    assert!(conn.target_state().is_stopped());
    assert!(!conn.frames().is_empty());

    // Now submit an exec command and have GDB reply with ^error. Per the
    // manual, target state becomes unknown.
    submit_and_ack(&mut conn, MiCommand::new("exec-continue"));
    conn.receive_bytes(b"1^error,msg=\"target not in runnable state\"\n(gdb)\n")
        .unwrap();
    drain(&mut conn);

    assert_eq!(*conn.target_state(), TargetState::Unknown);
    assert!(
        conn.frames().is_empty(),
        "frames must be cleared when target transitions to Unknown"
    );
}

#[test]
fn error_on_non_exec_command_does_not_reset_target() {
    let mut conn = Connection::new();
    conn.receive_bytes(
        b"*stopped,reason=\"breakpoint-hit\",thread-id=\"1\",\
          frame={level=\"0\",func=\"main\"}\n(gdb)\n",
    )
    .unwrap();
    drain(&mut conn);
    assert!(conn.target_state().is_stopped());

    submit_and_ack(&mut conn, MiCommand::new("break-insert").parameter("main"));
    conn.receive_bytes(b"1^error,msg=\"function not found\"\n(gdb)\n")
        .unwrap();
    drain(&mut conn);

    // A break-insert error does NOT invalidate target state.
    assert!(
        conn.target_state().is_stopped(),
        "non-exec error should leave target state alone"
    );
}

#[test]
fn exited_normally_transitions_to_exited() {
    let mut conn = Connection::new();
    conn.receive_bytes(b"*stopped,reason=\"exited-normally\",thread-id=\"1\"\n(gdb)\n")
        .unwrap();
    drain(&mut conn);
    assert_eq!(
        *conn.target_state(),
        TargetState::Exited { exit_code: Some(0) }
    );
    assert!(conn.target_state().is_exited());
    assert!(!conn.target_state().is_stopped());
}

#[test]
fn exited_with_code_transitions_to_exited() {
    let mut conn = Connection::new();
    conn.receive_bytes(b"*stopped,reason=\"exited\",exit-code=\"42\",thread-id=\"1\"\n(gdb)\n")
        .unwrap();
    drain(&mut conn);
    assert_eq!(
        *conn.target_state(),
        TargetState::Exited {
            exit_code: Some(42)
        }
    );
}

#[test]
fn exited_signalled_transitions_to_exited() {
    let mut conn = Connection::new();
    conn.receive_bytes(
        b"*stopped,reason=\"exited-signalled\",signal-name=\"SIGKILL\",thread-id=\"1\"\n(gdb)\n",
    )
    .unwrap();
    drain(&mut conn);
    assert_eq!(
        *conn.target_state(),
        TargetState::Exited { exit_code: None }
    );
}

#[test]
fn breakpoint_hit_extracts_bkptno() {
    let mut conn = Connection::new();
    conn.receive_bytes(
        b"*stopped,reason=\"breakpoint-hit\",disp=\"keep\",bkptno=\"1\",thread-id=\"1\",\
          frame={level=\"0\",addr=\"0x400500\",func=\"main\",file=\"hello.c\",\
          fullname=\"/tmp/hello.c\",line=\"3\"}\n(gdb)\n",
    )
    .unwrap();
    drain(&mut conn);
    match conn.target_state() {
        TargetState::Stopped { reason, .. } => {
            assert_eq!(
                *reason,
                Some(StoppedReason::BreakpointHit {
                    bkptno: Some("1".to_string()),
                })
            );
        }
        other => panic!("expected Stopped, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Thread registry
// ---------------------------------------------------------------------------

#[test]
fn thread_created_populates_registry() {
    let mut conn = Connection::new();
    conn.receive_bytes(b"=thread-created,id=\"1\",group-id=\"i1\"\n(gdb)\n")
        .unwrap();
    drain(&mut conn);
    assert_eq!(conn.threads().len(), 1);
    let info = conn
        .threads()
        .get(&ThreadId::new("1"))
        .expect("thread 1 should be present");
    assert_eq!(info.id, ThreadId::new("1"));
    assert_eq!(
        info.group_id.as_ref().map(ThreadGroupId::as_str),
        Some("i1")
    );
}

#[test]
fn thread_exited_removes_from_registry() {
    let mut conn = Connection::new();
    conn.receive_bytes(
        b"=thread-created,id=\"1\",group-id=\"i1\"\n\
          =thread-created,id=\"2\",group-id=\"i1\"\n(gdb)\n",
    )
    .unwrap();
    drain(&mut conn);
    assert_eq!(conn.threads().len(), 2);

    conn.receive_bytes(b"=thread-exited,id=\"1\",group-id=\"i1\"\n(gdb)\n")
        .unwrap();
    drain(&mut conn);
    assert_eq!(conn.threads().len(), 1);
    assert!(conn.threads().get(&ThreadId::new("1")).is_none());
    assert!(conn.threads().get(&ThreadId::new("2")).is_some());
}

#[test]
fn running_updates_thread_state() {
    let mut conn = Connection::new();
    conn.receive_bytes(
        b"=thread-created,id=\"1\",group-id=\"i1\"\n\
          *running,thread-id=\"1\"\n(gdb)\n",
    )
    .unwrap();
    drain(&mut conn);
    let info = conn.threads().get(&ThreadId::new("1")).expect("thread 1");
    assert_eq!(info.state, Some(ThreadState::Running));
}

// ---------------------------------------------------------------------------
// Frame registry
// ---------------------------------------------------------------------------

#[test]
fn stopped_record_updates_frame_registry() {
    let mut conn = Connection::new();
    conn.receive_bytes(
        b"*stopped,reason=\"breakpoint-hit\",thread-id=\"1\",\
          frame={level=\"0\",addr=\"0x400500\",func=\"main\",args=[],\
          file=\"hello.c\",fullname=\"/tmp/hello.c\",line=\"3\"}\n(gdb)\n",
    )
    .unwrap();
    drain(&mut conn);
    let frame = conn
        .frames()
        .current(&ThreadId::new("1"))
        .expect("frame for thread 1");
    assert_eq!(frame.level, Some(0));
    assert_eq!(frame.func.as_deref(), Some("main"));
    assert_eq!(frame.file.as_deref(), Some("hello.c"));
    assert_eq!(frame.fullname.as_deref(), Some("/tmp/hello.c"));
    assert_eq!(frame.line, Some(3));
    assert_eq!(frame.addr.as_deref(), Some("0x400500"));
}

#[test]
fn running_invalidates_frame_for_that_thread() {
    let mut conn = Connection::new();
    conn.receive_bytes(
        b"*stopped,reason=\"breakpoint-hit\",thread-id=\"1\",\
          frame={level=\"0\",func=\"main\"}\n(gdb)\n",
    )
    .unwrap();
    drain(&mut conn);
    assert!(conn.frames().current(&ThreadId::new("1")).is_some());

    conn.receive_bytes(b"*running,thread-id=\"1\"\n(gdb)\n")
        .unwrap();
    drain(&mut conn);
    assert!(
        conn.frames().current(&ThreadId::new("1")).is_none(),
        "running thread must have its frame invalidated"
    );
}

// ---------------------------------------------------------------------------
// Breakpoint registry
// ---------------------------------------------------------------------------

#[test]
fn break_insert_result_populates_breakpoint_registry() {
    let mut conn = Connection::new();
    submit_and_ack(&mut conn, MiCommand::new("break-insert").parameter("main"));
    conn.receive_bytes(
        b"1^done,bkpt={number=\"1\",type=\"breakpoint\",disp=\"keep\",enabled=\"y\",\
          addr=\"0x400500\",func=\"main\",file=\"hello.c\",fullname=\"/tmp/hello.c\",\
          line=\"3\",thread-groups=[\"i1\"],times=\"0\"}\n(gdb)\n",
    )
    .unwrap();
    drain(&mut conn);

    let bp = conn
        .breakpoints()
        .get(&BreakpointId::new("1"))
        .expect("breakpoint 1");
    assert_eq!(bp.id, BreakpointId::new("1"));
    assert_eq!(bp.kind.as_deref(), Some("breakpoint"));
    assert_eq!(bp.disp.as_deref(), Some("keep"));
    assert!(bp.enabled);
    assert_eq!(bp.times, Some(0));
    assert_eq!(bp.locations.len(), 1);
    assert_eq!(bp.locations[0].func.as_deref(), Some("main"));
    assert_eq!(bp.locations[0].file.as_deref(), Some("hello.c"));
    assert_eq!(bp.locations[0].fullname.as_deref(), Some("/tmp/hello.c"));
    assert_eq!(bp.locations[0].line, Some(3));
}

#[test]
fn mi3_multi_location_breakpoint_keeps_all_locations() {
    let mut conn = Connection::new();
    submit_and_ack(&mut conn, MiCommand::new("break-insert").parameter("foo"));
    conn.receive_bytes(
        b"1^done,bkpt={number=\"1\",type=\"breakpoint\",disp=\"keep\",enabled=\"y\",\
          times=\"0\",locations=[\
          {number=\"1.1\",enabled=\"y\",addr=\"0x400500\",func=\"foo\",file=\"a.c\",line=\"1\"},\
          {number=\"1.2\",enabled=\"y\",addr=\"0x400510\",func=\"foo\",file=\"b.c\",line=\"2\"}]}\n(gdb)\n",
    )
    .unwrap();
    drain(&mut conn);

    let bp = conn
        .breakpoints()
        .get(&BreakpointId::new("1"))
        .expect("breakpoint 1");
    assert_eq!(bp.locations.len(), 2);
    assert_eq!(bp.locations[0].number.as_deref(), Some("1.1"));
    assert_eq!(bp.locations[0].file.as_deref(), Some("a.c"));
    assert_eq!(bp.locations[0].line, Some(1));
    assert_eq!(bp.locations[1].number.as_deref(), Some("1.2"));
    assert_eq!(bp.locations[1].file.as_deref(), Some("b.c"));
    assert_eq!(bp.locations[1].line, Some(2));
}

#[test]
fn breakpoint_deleted_notification_removes_from_registry() {
    let mut conn = Connection::new();
    submit_and_ack(&mut conn, MiCommand::new("break-insert").parameter("main"));
    conn.receive_bytes(b"1^done,bkpt={number=\"1\",type=\"breakpoint\"}\n(gdb)\n")
        .unwrap();
    drain(&mut conn);
    assert_eq!(conn.breakpoints().len(), 1);

    conn.receive_bytes(b"=breakpoint-deleted,id=\"1\"\n(gdb)\n")
        .unwrap();
    drain(&mut conn);
    assert_eq!(conn.breakpoints().len(), 0);
}

#[test]
fn breakpoint_modified_notification_updates_registry() {
    let mut conn = Connection::new();
    submit_and_ack(&mut conn, MiCommand::new("break-insert").parameter("main"));
    conn.receive_bytes(b"1^done,bkpt={number=\"1\",type=\"breakpoint\",enabled=\"y\"}\n(gdb)\n")
        .unwrap();
    drain(&mut conn);

    // Disable it via an async =breakpoint-modified notification.
    conn.receive_bytes(
        b"=breakpoint-modified,bkpt={number=\"1\",type=\"breakpoint\",enabled=\"n\"}\n(gdb)\n",
    )
    .unwrap();
    drain(&mut conn);
    let bp = conn
        .breakpoints()
        .get(&BreakpointId::new("1"))
        .expect("bp 1");
    assert!(!bp.enabled);
}

// ---------------------------------------------------------------------------
// Variable object lifecycle
// ---------------------------------------------------------------------------

#[test]
fn var_create_result_populates_varobj_registry() {
    let mut conn = Connection::new();
    submit_and_ack(
        &mut conn,
        MiCommand::new("var-create")
            .parameter("-") // auto-allocate name
            .parameter("*") // frame expression
            .parameter("counter"), // the expression
    );
    conn.receive_bytes(
        b"1^done,name=\"var1\",numchild=\"0\",value=\"42\",type=\"int\",\
          thread-id=\"1\",has_more=\"0\"\n(gdb)\n",
    )
    .unwrap();
    drain(&mut conn);

    let vo = conn
        .varobjs()
        .get(&VarObjName::new("var1"))
        .expect("varobj var1");
    assert_eq!(vo.name, VarObjName::new("var1"));
    assert_eq!(vo.expression.as_deref(), Some("counter"));
    assert_eq!(vo.type_name.as_deref(), Some("int"));
    assert_eq!(vo.value.as_deref(), Some("42"));
    assert_eq!(vo.numchild, Some(0));
}

#[test]
fn var_update_result_refreshes_value() {
    let mut conn = Connection::new();
    submit_and_ack(
        &mut conn,
        MiCommand::new("var-create")
            .parameter("-")
            .parameter("*")
            .parameter("counter"),
    );
    conn.receive_bytes(b"1^done,name=\"var1\",value=\"1\",type=\"int\",numchild=\"0\"\n(gdb)\n")
        .unwrap();
    drain(&mut conn);

    submit_and_ack(
        &mut conn,
        MiCommand::new("var-update")
            .parameter("--all-values")
            .parameter("var1"),
    );
    conn.receive_bytes(
        b"2^done,changelist=[{name=\"var1\",value=\"42\",in_scope=\"true\",type_changed=\"false\"}]\n(gdb)\n",
    )
    .unwrap();
    drain(&mut conn);

    let vo = conn
        .varobjs()
        .get(&VarObjName::new("var1"))
        .expect("varobj var1");
    assert_eq!(vo.value.as_deref(), Some("42"));
    assert_eq!(vo.in_scope, Some(true));
}

#[test]
fn var_delete_removes_from_registry() {
    let mut conn = Connection::new();
    submit_and_ack(
        &mut conn,
        MiCommand::new("var-create")
            .parameter("-")
            .parameter("*")
            .parameter("x"),
    );
    conn.receive_bytes(b"1^done,name=\"var1\",value=\"0\",type=\"int\",numchild=\"0\"\n(gdb)\n")
        .unwrap();
    drain(&mut conn);
    assert_eq!(conn.varobjs().len(), 1);

    submit_and_ack(&mut conn, MiCommand::new("var-delete").parameter("var1"));
    conn.receive_bytes(b"2^done,ndeleted=\"1\"\n(gdb)\n")
        .unwrap();
    drain(&mut conn);
    assert_eq!(conn.varobjs().len(), 0);
}

// ---------------------------------------------------------------------------
// Feature cache
// ---------------------------------------------------------------------------

#[test]
fn list_features_populates_feature_cache() {
    let mut conn = Connection::new();
    submit_and_ack(&mut conn, MiCommand::new("list-features"));
    conn.receive_bytes(
        b"1^done,features=[\"frozen-varobjs\",\"pending-breakpoints\",\"python\",\"thread-info\"]\n(gdb)\n",
    )
    .unwrap();
    drain(&mut conn);

    assert!(conn.features().has("frozen-varobjs"));
    assert!(conn.features().has("python"));
    assert!(conn.features().has("thread-info"));
    assert!(!conn.features().has("nonexistent-feature"));
}

#[test]
fn list_target_features_populates_separate_cache() {
    let mut conn = Connection::new();
    submit_and_ack(&mut conn, MiCommand::new("list-target-features"));
    conn.receive_bytes(b"1^done,features=[\"async\",\"reverse\"]\n(gdb)\n")
        .unwrap();
    drain(&mut conn);

    assert!(conn.features().target_has("async"));
    assert!(conn.features().target_has("reverse"));
    // The general features set is untouched.
    assert!(!conn.features().has("async"));
}

// ---------------------------------------------------------------------------
// End-to-end: a representative debugging session replay
// ---------------------------------------------------------------------------

#[test]
fn realistic_session_replay_updates_all_registries() {
    let mut conn = Connection::new();

    // 1. Thread group added during GDB startup.
    conn.receive_bytes(b"=thread-group-added,id=\"i1\"\n(gdb)\n")
        .unwrap();
    // 2. Load a file and insert a breakpoint at main.
    submit_and_ack(&mut conn, MiCommand::new("break-insert").parameter("main"));
    conn.receive_bytes(
        b"1^done,bkpt={number=\"1\",type=\"breakpoint\",disp=\"keep\",enabled=\"y\",\
          addr=\"0x400500\",func=\"main\",file=\"hello.c\",fullname=\"/tmp/hello.c\",\
          line=\"3\",times=\"0\"}\n(gdb)\n",
    )
    .unwrap();
    // 3. Run the target.
    submit_and_ack(&mut conn, MiCommand::new("exec-run"));
    conn.receive_bytes(b"2^running\n").unwrap();
    // 4. Thread created as the target starts.
    conn.receive_bytes(b"=thread-created,id=\"1\",group-id=\"i1\"\n")
        .unwrap();
    // 5. Target running.
    conn.receive_bytes(b"*running,thread-id=\"all\"\n(gdb)\n")
        .unwrap();
    // 6. Later: breakpoint hit.
    conn.receive_bytes(
        b"*stopped,reason=\"breakpoint-hit\",disp=\"keep\",bkptno=\"1\",thread-id=\"1\",\
          frame={level=\"0\",addr=\"0x400500\",func=\"main\",args=[],\
          file=\"hello.c\",fullname=\"/tmp/hello.c\",line=\"3\"}\n(gdb)\n",
    )
    .unwrap();
    drain(&mut conn);

    // All the registries should reflect the expected state now.
    assert!(conn.target_state().is_stopped());
    assert_eq!(conn.threads().len(), 1);
    assert!(conn.threads().get(&ThreadId::new("1")).is_some());
    assert_eq!(conn.breakpoints().len(), 1);
    let bp = conn
        .breakpoints()
        .get(&BreakpointId::new("1"))
        .expect("breakpoint 1");
    assert_eq!(bp.locations.len(), 1);
    let frame = conn
        .frames()
        .current(&ThreadId::new("1"))
        .expect("frame for thread 1");
    assert_eq!(frame.func.as_deref(), Some("main"));
    assert_eq!(frame.line, Some(3));
}
