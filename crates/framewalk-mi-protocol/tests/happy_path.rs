//! Step 3 milestone tests: drive a `Connection` through realistic GDB
//! exchanges using canned byte streams. No I/O, no actual GDB — pure
//! sans-IO state machine verification.

use framewalk_mi_codec::{MiCommand, Token, Value};
use framewalk_mi_protocol::{
    CommandHandle, CommandOutcome, CommandRequest, Connection, Event, NotifyEvent, RunningEvent,
    StoppedEvent, StoppedReason, ThreadId,
};

/// Drain every event currently queued, returning them in arrival order.
fn drain_events(conn: &mut Connection) -> Vec<Event> {
    let mut out = Vec::new();
    while let Some(e) = conn.poll_event() {
        out.push(e);
    }
    out
}

/// Helper that submits a command, assertion-checks the bytes the connection
/// wrote into its outbound buffer, acknowledges them, and returns the
/// allocated handle.
fn submit_and_ack(conn: &mut Connection, cmd: MiCommand, expected_wire: &[u8]) -> CommandHandle {
    let handle = conn.submit(CommandRequest::new(cmd));
    assert_eq!(
        conn.outbound(),
        expected_wire,
        "outbound wire bytes mismatch"
    );
    let n = conn.outbound().len();
    conn.consume_outbound(n).unwrap();
    assert_eq!(conn.outbound().len(), 0);
    handle
}

// ---------------------------------------------------------------------------
// Basic correlation
// ---------------------------------------------------------------------------

#[test]
fn gdb_version_roundtrip() {
    let mut conn = Connection::new();
    let handle = submit_and_ack(&mut conn, MiCommand::new("gdb-version"), b"1-gdb-version\n");

    conn.receive_bytes(b"~\"GNU gdb (GDB) 15.1\\n\"\n1^done\n(gdb)\n")
        .unwrap();

    let events = drain_events(&mut conn);
    assert_eq!(events.len(), 3, "expected 3 events, got {events:#?}");

    match &events[0] {
        Event::Console(text) => assert_eq!(text, "GNU gdb (GDB) 15.1\n"),
        other => panic!("expected Console, got {other:?}"),
    }
    match &events[1] {
        Event::CommandCompleted {
            handle: h,
            outcome: CommandOutcome::Done(results),
        } => {
            assert_eq!(*h, handle);
            assert!(results.is_empty(), "expected bare ^done, got {results:?}");
        }
        other => panic!("expected CommandCompleted(Done), got {other:?}"),
    }
    assert!(matches!(events[2], Event::GroupClosed));
}

#[test]
fn list_features_done_carries_result_tuple() {
    let mut conn = Connection::new();
    let handle = submit_and_ack(
        &mut conn,
        MiCommand::new("list-features"),
        b"1-list-features\n",
    );

    conn.receive_bytes(
        b"1^done,features=[\"frozen-varobjs\",\"pending-breakpoints\",\"python\"]\n(gdb)\n",
    )
    .unwrap();

    let events = drain_events(&mut conn);
    assert_eq!(events.len(), 2);

    match &events[0] {
        Event::CommandCompleted {
            handle: h,
            outcome: CommandOutcome::Done(results),
        } => {
            assert_eq!(*h, handle);
            assert_eq!(results[0].0, "features");
            // Inspect the features list to prove the full AST made it through.
            if let Value::List(framewalk_mi_codec::ListValue::Values(vs)) = &results[0].1 {
                assert_eq!(vs.len(), 3);
            } else {
                panic!("expected a value list for features");
            }
        }
        other => panic!("expected CommandCompleted(Done), got {other:?}"),
    }
    assert!(matches!(events[1], Event::GroupClosed));
}

#[test]
fn break_insert_result_carries_breakpoint_tuple() {
    let mut conn = Connection::new();
    let handle = submit_and_ack(
        &mut conn,
        MiCommand::new("break-insert").parameter("main"),
        b"1-break-insert main\n",
    );

    conn.receive_bytes(
        b"1^done,bkpt={number=\"1\",type=\"breakpoint\",disp=\"keep\",\
          enabled=\"y\",addr=\"0x400500\",func=\"main\",file=\"hello.c\",\
          fullname=\"/tmp/hello.c\",line=\"3\",thread-groups=[\"i1\"],times=\"0\"}\n(gdb)\n",
    )
    .unwrap();

    let events = drain_events(&mut conn);
    assert_eq!(events.len(), 2);
    match &events[0] {
        Event::CommandCompleted {
            handle: h,
            outcome: CommandOutcome::Done(results),
        } => {
            assert_eq!(*h, handle);
            let (k, v) = &results[0];
            assert_eq!(k, "bkpt");
            assert!(matches!(v, Value::Tuple(_)));
        }
        other => panic!("expected CommandCompleted(Done), got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Stop/continue cycle: the `^running` vs `*stopped` distinction
// ---------------------------------------------------------------------------

#[test]
fn exec_run_then_stopped_cycle() {
    let mut conn = Connection::new();
    let run_handle = submit_and_ack(&mut conn, MiCommand::new("exec-run"), b"1-exec-run\n");

    // GDB's reply: ^running (command accepted) then an independent
    // *stopped async record inside its own response group later.
    conn.receive_bytes(
        b"1^running\n*running,thread-id=\"all\"\n(gdb)\n\
          *stopped,reason=\"breakpoint-hit\",disp=\"keep\",bkptno=\"1\",\
          thread-id=\"1\",frame={addr=\"0x400500\",func=\"main\",args=[],\
          file=\"hello.c\",fullname=\"/tmp/hello.c\",line=\"3\"}\n(gdb)\n",
    )
    .unwrap();

    let events = drain_events(&mut conn);

    // Expected sequence:
    //   1. CommandCompleted(Running) — the command itself is done.
    //   2. Running(thread="all") — the async status transition.
    //   3. GroupClosed — first (gdb) prompt.
    //   4. Stopped — the target halted.
    //   5. GroupClosed — second (gdb) prompt.
    assert_eq!(events.len(), 5, "got: {events:#?}");

    match &events[0] {
        Event::CommandCompleted {
            handle: h,
            outcome: CommandOutcome::Running,
        } => {
            assert_eq!(*h, run_handle);
        }
        other => panic!(
            "expected CommandCompleted(Running) — this is the biggest MI \
             deadlock trap: ^running MUST complete the command, the later \
             *stopped is a separate event. Got: {other:?}"
        ),
    }
    match &events[1] {
        Event::Running(RunningEvent { thread }) => {
            assert_eq!(thread.as_ref().map(ThreadId::as_str), Some("all"));
        }
        other => panic!("expected Running async, got {other:?}"),
    }
    assert!(matches!(events[2], Event::GroupClosed));
    match &events[3] {
        Event::Stopped(StoppedEvent { reason, thread, .. }) => {
            assert!(matches!(reason, Some(StoppedReason::BreakpointHit { .. })));
            assert_eq!(thread.as_ref().map(ThreadId::as_str), Some("1"));
        }
        other => panic!("expected Stopped, got {other:?}"),
    }
    assert!(matches!(events[4], Event::GroupClosed));
}

// ---------------------------------------------------------------------------
// Error outcomes and async notifications
// ---------------------------------------------------------------------------

#[test]
fn error_outcome_carries_msg_and_code() {
    let mut conn = Connection::new();
    let handle = submit_and_ack(
        &mut conn,
        MiCommand::new("bogus-command"),
        b"1-bogus-command\n",
    );

    conn.receive_bytes(b"1^error,msg=\"no such command\",code=\"undefined-command\"\n(gdb)\n")
        .unwrap();

    let events = drain_events(&mut conn);
    match &events[0] {
        Event::CommandCompleted {
            handle: h,
            outcome: CommandOutcome::Error { msg, code },
        } => {
            assert_eq!(*h, handle);
            assert_eq!(msg, "no such command");
            assert_eq!(code.as_deref(), Some("undefined-command"));
        }
        other => panic!("expected Error outcome, got {other:?}"),
    }
}

#[test]
fn notify_async_is_emitted_as_notify_event() {
    let mut conn = Connection::new();
    conn.receive_bytes(b"=thread-created,id=\"1\",group-id=\"i1\"\n(gdb)\n")
        .unwrap();
    let events = drain_events(&mut conn);
    assert_eq!(events.len(), 2);
    match &events[0] {
        Event::Notify(NotifyEvent { class, results }) => {
            assert_eq!(class, "thread-created");
            assert_eq!(results[0].0, "id");
            assert_eq!(results[1].0, "group-id");
        }
        other => panic!("expected Notify, got {other:?}"),
    }
    assert!(matches!(events[1], Event::GroupClosed));
}

// ---------------------------------------------------------------------------
// Untokened results and unknown async classes
// ---------------------------------------------------------------------------

#[test]
fn untokened_result_record_is_unknown() {
    let mut conn = Connection::new();
    conn.receive_bytes(b"^done\n(gdb)\n").unwrap();
    let events = drain_events(&mut conn);
    assert_eq!(events.len(), 2);
    assert!(matches!(events[0], Event::Unknown(_)));
    assert!(matches!(events[1], Event::GroupClosed));
}

#[test]
fn unknown_exec_class_passes_through_as_unknown() {
    // A hypothetical future async class framewalk has never seen — must
    // not reject, must pass through.
    let mut conn = Connection::new();
    conn.receive_bytes(b"*framewalk-invented,foo=\"bar\"\n(gdb)\n")
        .unwrap();
    let events = drain_events(&mut conn);
    assert!(matches!(events[0], Event::Unknown(_)));
}

// ---------------------------------------------------------------------------
// Parse errors surface as events, not Err returns
// ---------------------------------------------------------------------------

#[test]
fn malformed_line_surfaces_as_parse_error_event() {
    let mut conn = Connection::new();
    // `?bad` has an invalid record prefix.
    conn.receive_bytes(b"?bad\n").unwrap();
    let events = drain_events(&mut conn);
    assert_eq!(events.len(), 1);
    match &events[0] {
        Event::ParseError(failure) => {
            assert_eq!(failure.raw_line, b"?bad");
        }
        other => panic!("expected ParseError, got {other:?}"),
    }
}

#[test]
fn parse_error_does_not_kill_session() {
    // A bad line in the middle of a stream should not prevent subsequent
    // good lines from parsing.
    let mut conn = Connection::new();
    conn.receive_bytes(b"?bad\n^done\n(gdb)\n").unwrap();
    let events = drain_events(&mut conn);
    assert_eq!(events.len(), 3);
    assert!(matches!(events[0], Event::ParseError(_)));
    // Untokened ^done → Unknown, not CommandCompleted.
    assert!(matches!(events[1], Event::Unknown(_)));
    assert!(matches!(events[2], Event::GroupClosed));
}

// ---------------------------------------------------------------------------
// Byte-level streaming: chunked input
// ---------------------------------------------------------------------------

#[test]
fn handles_receive_bytes_in_arbitrary_chunks() {
    let mut conn = Connection::new();
    let handle = submit_and_ack(&mut conn, MiCommand::new("gdb-version"), b"1-gdb-version\n");

    let full = b"1^done,version=\"15.1\"\n(gdb)\n";
    // Feed the bytes one at a time.
    for &b in full {
        conn.receive_bytes(&[b]).unwrap();
    }

    let events = drain_events(&mut conn);
    assert_eq!(events.len(), 2);
    match &events[0] {
        Event::CommandCompleted {
            handle: h,
            outcome: CommandOutcome::Done(results),
        } => {
            assert_eq!(*h, handle);
            assert_eq!(results[0].0, "version");
        }
        other => panic!("expected CommandCompleted, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Token allocation correctness
// ---------------------------------------------------------------------------

#[test]
fn sequential_submits_allocate_sequential_tokens() {
    let mut conn = Connection::new();
    let h1 = conn.submit(CommandRequest::new(MiCommand::new("a")));
    let h2 = conn.submit(CommandRequest::new(MiCommand::new("b")));
    let h3 = conn.submit(CommandRequest::new(MiCommand::new("c")));
    assert_eq!(h1.token(), Token::new(1));
    assert_eq!(h2.token(), Token::new(2));
    assert_eq!(h3.token(), Token::new(3));
    // All three commands concatenated in the outbound buffer, in order.
    assert_eq!(conn.outbound(), b"1-a\n2-b\n3-c\n");
}

#[test]
fn consume_outbound_advances_cursor() {
    let mut conn = Connection::new();
    conn.submit(CommandRequest::new(MiCommand::new("first")));
    conn.submit(CommandRequest::new(MiCommand::new("second")));
    assert_eq!(conn.outbound(), b"1-first\n2-second\n");
    // Simulate writing only the first command.
    conn.consume_outbound(b"1-first\n".len()).unwrap();
    assert_eq!(conn.outbound(), b"2-second\n");
    conn.consume_outbound(conn.outbound().len()).unwrap();
    assert_eq!(conn.outbound().len(), 0);
}
