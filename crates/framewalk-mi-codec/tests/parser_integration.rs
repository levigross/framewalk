//! End-to-end tests for the mi-codec parser and encoder against realistic
//! MI v3 output samples. These exercise the whole parser path including
//! record dispatch, value recursion, c-string escape decoding, token
//! correlation, and the encoder's quoting rules.
//!
//! Every sample is either hand-crafted from the BNF in the GDB manual or
//! lightly adapted from the manual's own example output. No existing MI
//! implementation was consulted.

use framewalk_mi_codec::{
    encode_command, parse_record, AsyncClass, AsyncRecord, CodecErrorKind, ListValue, MiCommand,
    Record, ResultClass, ResultRecord, Token, Value,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse(input: &str) -> Record {
    parse_record(input.as_bytes())
        .unwrap_or_else(|e| panic!("expected {input:?} to parse; got error {e}"))
}

fn parse_err(input: &str) -> framewalk_mi_codec::CodecError {
    parse_record(input.as_bytes())
        .err()
        .unwrap_or_else(|| panic!("expected {input:?} to fail parsing, but it succeeded"))
}

fn c(s: &str) -> Value {
    Value::Const(s.into())
}

fn results(pairs: &[(&str, Value)]) -> Vec<(String, Value)> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).into(), v.clone()))
        .collect()
}

// ---------------------------------------------------------------------------
// Result records: ^done, ^running, ^connected, ^error, ^exit
// ---------------------------------------------------------------------------

#[test]
fn parses_bare_done() {
    let r = parse("^done");
    assert_eq!(
        r,
        Record::Result(ResultRecord {
            token: None,
            class: ResultClass::Done,
            results: vec![],
        })
    );
}

#[test]
fn parses_done_with_single_result() {
    let r = parse("^done,value=\"42\"");
    assert_eq!(
        r,
        Record::Result(ResultRecord {
            token: None,
            class: ResultClass::Done,
            results: results(&[("value", c("42"))]),
        })
    );
}

#[test]
fn parses_done_with_multiple_results() {
    let r = parse("^done,a=\"1\",b=\"two\",c=\"3\"");
    if let Record::Result(rr) = r {
        assert_eq!(rr.class, ResultClass::Done);
        assert_eq!(
            rr.results,
            results(&[("a", c("1")), ("b", c("two")), ("c", c("3"))])
        );
    } else {
        panic!("expected ResultRecord");
    }
}

#[test]
fn parses_running() {
    let r = parse("^running");
    assert!(matches!(
        r,
        Record::Result(ResultRecord {
            class: ResultClass::Running,
            ..
        })
    ));
}

#[test]
fn parses_connected() {
    let r = parse("^connected");
    assert!(matches!(
        r,
        Record::Result(ResultRecord {
            class: ResultClass::Connected,
            ..
        })
    ));
}

#[test]
fn parses_exit() {
    let r = parse("^exit");
    assert!(matches!(
        r,
        Record::Result(ResultRecord {
            class: ResultClass::Exit,
            ..
        })
    ));
}

#[test]
fn parses_error_with_msg_only() {
    let r = parse("^error,msg=\"nothing works\"");
    if let Record::Result(rr) = r {
        assert_eq!(rr.class, ResultClass::Error);
        assert_eq!(rr.results, results(&[("msg", c("nothing works"))]));
    } else {
        panic!("expected ResultRecord");
    }
}

#[test]
fn parses_error_with_msg_and_code() {
    let r = parse("^error,msg=\"no such command\",code=\"undefined-command\"");
    if let Record::Result(rr) = r {
        assert_eq!(
            rr.results,
            results(&[
                ("msg", c("no such command")),
                ("code", c("undefined-command")),
            ])
        );
    } else {
        panic!();
    }
}

#[test]
fn parses_result_with_token() {
    let r = parse("42^done,value=\"x\"");
    if let Record::Result(rr) = r {
        assert_eq!(rr.token, Some(Token::new(42)));
        assert_eq!(rr.class, ResultClass::Done);
    } else {
        panic!();
    }
}

#[test]
fn rejects_unknown_result_class() {
    let err = parse_err("^bogus");
    assert!(matches!(
        err.kind,
        CodecErrorKind::UnknownResultClass { .. }
    ));
}

// ---------------------------------------------------------------------------
// Async records: *, +, =
// ---------------------------------------------------------------------------

#[test]
fn parses_bare_exec_running() {
    let r = parse("*running,thread-id=\"all\"");
    assert_eq!(
        r,
        Record::Exec(AsyncRecord {
            token: None,
            class: AsyncClass::new("running"),
            results: results(&[("thread-id", c("all"))]),
        })
    );
}

#[test]
fn parses_exec_stopped_with_frame() {
    // A representative *stopped record with nested tuples for `frame`.
    let r = parse(
        "*stopped,reason=\"breakpoint-hit\",disp=\"keep\",bkptno=\"1\",\
         frame={addr=\"0x4005a0\",func=\"main\",args=[],\
         file=\"hello.c\",fullname=\"/tmp/hello.c\",line=\"3\"}",
    );
    if let Record::Exec(ar) = r {
        assert_eq!(ar.class, AsyncClass::new("stopped"));
        assert_eq!(ar.results[0], ("reason".into(), c("breakpoint-hit")));
        assert_eq!(ar.results[1], ("disp".into(), c("keep")));
        assert_eq!(ar.results[2], ("bkptno".into(), c("1")));
        assert_eq!(ar.results[3].0, "frame");
        let frame = &ar.results[3].1;
        if let Value::Tuple(frame_results) = frame {
            let func = frame_results.iter().find(|(k, _)| k == "func").unwrap();
            assert_eq!(func.1, c("main"));
        } else {
            panic!("expected frame to be a Tuple; got {frame:?}");
        }
    } else {
        panic!("expected Exec record");
    }
}

#[test]
fn parses_notify_thread_created() {
    let r = parse("=thread-created,id=\"1\",group-id=\"i1\"");
    if let Record::Notify(ar) = r {
        assert_eq!(ar.class, AsyncClass::new("thread-created"));
        assert_eq!(
            ar.results,
            results(&[("id", c("1")), ("group-id", c("i1"))])
        );
    } else {
        panic!();
    }
}

#[test]
fn parses_notify_unknown_class_is_not_an_error() {
    // Forward-compat: a class we've never seen must parse without error.
    // This is the rationale for `AsyncClass` being a newtyped String rather
    // than an enum.
    let r = parse("=framewalk-invented-class,foo=\"bar\"");
    if let Record::Notify(ar) = r {
        assert_eq!(ar.class.as_str(), "framewalk-invented-class");
    } else {
        panic!();
    }
}

#[test]
fn parses_status_record() {
    let r = parse("+download,section=\".text\"");
    assert!(matches!(r, Record::Status(_)));
}

#[test]
fn parses_async_with_token() {
    let r = parse("7*stopped");
    if let Record::Exec(ar) = r {
        assert_eq!(ar.token, Some(Token::new(7)));
        assert_eq!(ar.class, AsyncClass::new("stopped"));
    } else {
        panic!();
    }
}

// ---------------------------------------------------------------------------
// Stream records: ~, @, &
// ---------------------------------------------------------------------------

#[test]
fn parses_console_stream() {
    let r = parse("~\"GNU gdb (GDB) 15.1\\n\"");
    if let Record::Console(sr) = r {
        assert_eq!(sr.text, "GNU gdb (GDB) 15.1\n");
    } else {
        panic!();
    }
}

#[test]
fn parses_target_stream() {
    let r = parse("@\"hello from target\\n\"");
    if let Record::Target(sr) = r {
        assert_eq!(sr.text, "hello from target\n");
    } else {
        panic!();
    }
}

#[test]
fn parses_log_stream() {
    let r = parse("&\"debug: continuing\\n\"");
    if let Record::Log(sr) = r {
        assert_eq!(sr.text, "debug: continuing\n");
    } else {
        panic!();
    }
}

#[test]
fn rejects_stream_with_token() {
    // The BNF forbids tokens on stream records.
    let err = parse_err("123~\"console output\"");
    assert_eq!(err.kind, CodecErrorKind::StreamRecordHasToken);
}

// ---------------------------------------------------------------------------
// Value grammar: tuples, lists, nested values
// ---------------------------------------------------------------------------

#[test]
fn parses_empty_tuple() {
    let r = parse("^done,empty={}");
    if let Record::Result(rr) = r {
        assert_eq!(rr.results[0].1, Value::Tuple(vec![]));
    } else {
        panic!();
    }
}

#[test]
fn parses_nested_tuple() {
    let r = parse("^done,outer={inner={leaf=\"deep\"}}");
    if let Record::Result(rr) = r {
        let outer = &rr.results[0].1;
        if let Value::Tuple(pairs) = outer {
            let (k, v) = &pairs[0];
            assert_eq!(k, "inner");
            if let Value::Tuple(inner_pairs) = v {
                assert_eq!(inner_pairs[0].0, "leaf");
                assert_eq!(inner_pairs[0].1, c("deep"));
            } else {
                panic!("expected nested Tuple");
            }
        } else {
            panic!();
        }
    }
}

#[test]
fn parses_empty_list() {
    let r = parse("^done,items=[]");
    if let Record::Result(rr) = r {
        assert_eq!(rr.results[0].1, Value::List(ListValue::Empty));
    } else {
        panic!();
    }
}

#[test]
fn parses_list_of_values() {
    let r = parse("^done,items=[\"a\",\"b\",\"c\"]");
    if let Record::Result(rr) = r {
        if let Value::List(ListValue::Values(vs)) = &rr.results[0].1 {
            assert_eq!(*vs, vec![c("a"), c("b"), c("c")]);
        } else {
            panic!("expected ListValue::Values");
        }
    }
}

#[test]
fn parses_list_of_results() {
    let r = parse("^done,bkpts=[bkpt={number=\"1\",type=\"breakpoint\"},bkpt={number=\"2\",type=\"breakpoint\"}]");
    if let Record::Result(rr) = r {
        if let Value::List(ListValue::Results(pairs)) = &rr.results[0].1 {
            assert_eq!(pairs.len(), 2);
            assert_eq!(pairs[0].0, "bkpt");
            assert_eq!(pairs[1].0, "bkpt");
        } else {
            panic!("expected ListValue::Results");
        }
    }
}

#[test]
fn parses_list_of_tuples_as_values() {
    // A bare-value list whose values happen to be tuples.
    let r = parse("^done,frames=[{level=\"0\"},{level=\"1\"}]");
    if let Record::Result(rr) = r {
        if let Value::List(ListValue::Values(vs)) = &rr.results[0].1 {
            assert_eq!(vs.len(), 2);
            assert!(matches!(&vs[0], Value::Tuple(_)));
        } else {
            panic!();
        }
    }
}

#[test]
fn rejects_mixed_list() {
    // `[value, name=value]` is forbidden by the BNF.
    let err = parse_err("^done,mix=[\"a\",k=\"v\"]");
    assert_eq!(err.kind, CodecErrorKind::MixedList);
}

#[test]
fn parses_deeply_nested_list() {
    let r = parse("^done,x=[[[\"innermost\"]]]");
    if let Record::Result(rr) = r {
        if let Value::List(ListValue::Values(l1)) = &rr.results[0].1 {
            if let Value::List(ListValue::Values(l2)) = &l1[0] {
                if let Value::List(ListValue::Values(l3)) = &l2[0] {
                    assert_eq!(l3[0], c("innermost"));
                    return;
                }
            }
        }
        panic!("shape mismatch");
    }
}

// ---------------------------------------------------------------------------
// mi3 multi-location breakpoint shape
// ---------------------------------------------------------------------------

#[test]
fn parses_mi3_multi_location_breakpoint() {
    // The distinguishing mi3 shape: a single bkpt tuple with a nested
    // `locations=[...]` list-of-results.
    let r = parse(
        "^done,bkpt={number=\"1\",type=\"breakpoint\",\
         locations=[{number=\"1.1\",addr=\"0x400500\",func=\"foo\"},\
         {number=\"1.2\",addr=\"0x400510\",func=\"bar\"}]}",
    );
    if let Record::Result(rr) = r {
        let Value::Tuple(bkpt) = &rr.results[0].1 else {
            panic!("expected tuple for bkpt, got {:?}", rr.results[0].1);
        };
        let (_, locs) = bkpt
            .iter()
            .find(|(k, _)| k == "locations")
            .expect("locations key present");
        // `locations=[{...},{...}]` is a list of bare tuple values (no
        // `key=` prefix before each `{`), so it parses as Values, not
        // Results. The protocol layer will unwrap this into a typed
        // Vec<BreakpointLocation> in Step 4.
        let Value::List(ListValue::Values(vs)) = locs else {
            panic!("expected Values list for locations, got {locs:?}");
        };
        assert_eq!(vs.len(), 2);
        assert!(matches!(&vs[0], Value::Tuple(_)));
        assert!(matches!(&vs[1], Value::Tuple(_)));
    }
}

// ---------------------------------------------------------------------------
// Edge cases / errors
// ---------------------------------------------------------------------------

#[test]
fn rejects_empty_input() {
    let err = parse_err("");
    assert_eq!(err.kind, CodecErrorKind::UnexpectedEnd);
}

#[test]
fn rejects_bad_prefix() {
    let err = parse_err("?done");
    assert!(matches!(
        err.kind,
        CodecErrorKind::InvalidRecordPrefix { found: b'?' }
    ));
}

#[test]
fn rejects_trailing_garbage_after_result() {
    let err = parse_err("^done,a=\"1\"XXX");
    // The stray `X` after the first result should be flagged.
    assert!(matches!(
        err.kind,
        CodecErrorKind::UnexpectedByte { .. } | CodecErrorKind::TrailingGarbage
    ));
}

#[test]
fn rejects_missing_equals_in_result() {
    let err = parse_err("^done,bare");
    // The trailing `bare` without `=value` should be an ExpectedEquals or
    // EOF-reached-while-parsing-value error.
    assert!(matches!(
        err.kind,
        CodecErrorKind::ExpectedEquals | CodecErrorKind::UnexpectedEnd
    ));
}

#[test]
fn token_overflow_is_rejected() {
    // One more than u64::MAX.
    let err = parse_err("18446744073709551616^done");
    assert_eq!(err.kind, CodecErrorKind::TokenOverflow);
}

// ---------------------------------------------------------------------------
// Encoder
// ---------------------------------------------------------------------------

fn encode(token: Option<u64>, cmd: &MiCommand) -> String {
    let mut out: Vec<u8> = Vec::new();
    encode_command(token.map(Token::new), cmd, &mut out);
    String::from_utf8(out).expect("encoder output is always UTF-8")
}

#[test]
fn encode_simple_command() {
    assert_eq!(encode(None, &MiCommand::new("exec-run")), "-exec-run\n");
}

#[test]
fn encode_command_with_token() {
    assert_eq!(
        encode(Some(42), &MiCommand::new("exec-run")),
        "42-exec-run\n"
    );
}

#[test]
fn encode_command_with_bare_parameter() {
    assert_eq!(
        encode(None, &MiCommand::new("break-insert").parameter("main")),
        "-break-insert main\n"
    );
}

#[test]
fn encode_command_quotes_parameter_with_space() {
    assert_eq!(
        encode(
            None,
            &MiCommand::new("file-exec-and-symbols").parameter("/tmp/my program")
        ),
        "-file-exec-and-symbols \"/tmp/my program\"\n"
    );
}

#[test]
fn encode_command_escapes_quotes_in_parameter() {
    assert_eq!(
        encode(
            None,
            &MiCommand::new("data-evaluate-expression").parameter("x == \"y\"")
        ),
        "-data-evaluate-expression \"x == \\\"y\\\"\"\n"
    );
}

#[test]
fn encode_command_with_option_no_value() {
    assert_eq!(
        encode(
            None,
            &MiCommand::new("break-insert").option("t").parameter("main")
        ),
        "-break-insert -t main\n"
    );
}

#[test]
fn encode_command_with_option_value() {
    assert_eq!(
        encode(
            None,
            &MiCommand::new("break-insert")
                .option_with("condition", "x > 5")
                .parameter("main")
        ),
        "-break-insert -condition \"x > 5\" main\n"
    );
}

#[test]
fn encode_command_inserts_double_dash_for_dash_prefixed_parameter() {
    // `-break-insert -flag` would be ambiguous without `--`.
    assert_eq!(
        encode(None, &MiCommand::new("break-insert").parameter("-flag")),
        "-break-insert -- -flag\n"
    );
}

#[test]
fn encode_command_empty_parameter_gets_quoted() {
    // An empty string is not a valid non-blank-sequence; must be quoted.
    assert_eq!(
        encode(
            None,
            &MiCommand::new("data-evaluate-expression").parameter("")
        ),
        "-data-evaluate-expression \"\"\n"
    );
}

// ---------------------------------------------------------------------------
// Round-trip parser ↔ encoder for c-strings (via stream records)
// ---------------------------------------------------------------------------

#[test]
fn stream_record_roundtrip_through_encode_cstring() {
    // Parse a stream record, re-encode its text, and assert the re-encoded
    // form re-parses to the same text. This verifies that the encoder's
    // canonical form is always a valid input to the parser.
    let originals = [
        "",
        "plain",
        "hello\nworld",
        "tabs\tand\tquotes \"here\"",
        "backslash \\\\ escape",
        "bell\u{07} and null\u{00}",
        "utf8: café 日本語",
    ];
    for original in originals {
        let mut encoded: Vec<u8> = Vec::new();
        framewalk_mi_codec::encode::cstring::encode_cstring(original, &mut encoded);
        // The encoded bytes form a complete c-string literal; wrap with
        // a dummy stream-record prefix so we can reuse parse_record.
        let mut line: Vec<u8> = Vec::new();
        line.push(b'~');
        line.extend_from_slice(&encoded);
        let rec = parse_record(&line)
            .unwrap_or_else(|e| panic!("round-trip failed for {original:?}: {e}"));
        if let Record::Console(sr) = rec {
            assert_eq!(sr.text, original, "round-trip mismatch for {original:?}");
        } else {
            panic!("expected Console stream record");
        }
    }
}
