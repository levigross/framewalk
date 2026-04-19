# Scheme Reference

Complete reference for the Steel Scheme environment available through
`scheme_eval`. Steel implements R5RS Scheme with extensions — see the
[Steel documentation](https://github.com/mattwparas/steel) for the
full language.

This page covers the framewalk-specific primitives and prelude.

## Engine behaviour

- **State persists** across `scheme_eval` calls. Variables, function
  definitions, and module imports survive between invocations.
- **Errors** in Scheme (syntax errors, runtime exceptions, GDB errors)
  are returned as tool errors — they do not crash the engine.
- **Successful results are structured JSON.** `scheme_eval` serializes
  Scheme values into compact JSON rather than Steel's multiline printer.
  Leave `include_streams` off unless you explicitly want inline logs;
  otherwise use `drain_events` after the call.
- **Operator escape hatches stay available in scheme mode.**
  `interrupt_target`, `target_state`, `drain_events`, and
  `reconnect_target` are separate MCP tools, so a blocked or long-lived
  `scheme_eval` is not your only control surface.
- **Timeouts are configurable.** `scheme_eval` uses the server-wide
  `--scheme-eval-timeout-secs` / `FRAMEWALK_SCHEME_EVAL_TIMEOUT_SECS`
  default (60 seconds unless overridden). Wait helpers use
  `--wait-for-stop-timeout-secs` /
  `FRAMEWALK_WAIT_FOR_STOP_TIMEOUT_SECS` (30 seconds unless overridden).
  **Budget interaction:** a per-call wait timeout that exceeds the
  remaining `scheme_eval` budget raises an error immediately rather than
  being silently killed at the outer boundary. For long waits, increase
  `--scheme-eval-timeout-secs`.

## Rust primitives

These are the four functions implemented in Rust and registered into
the Scheme environment. Everything else is built on top of them.

### `(mi command-string)` → result-entry-list | symbol

Submit a raw GDB/MI command string and return the parsed result.

```scheme
(mi "-gdb-version")          ;; => empty result-entry list; banner is a console stream
(mi "-break-insert main")    ;; => result-entry list
(mi "-exec-run")             ;; => 'running (symbol)
```

**Returns:**
- `^done` responses → result-entry list (ordered, duplicate-preserving)
- `^running` responses → the symbol `running`
- `^connected` responses → result-entry list (same as done)
- `^error` responses → raises a Scheme error with the GDB message

**Security:** commands are validated against an allowlist of known MI
command families. Unrecognised families (including `-interpreter-exec`
and `-target-exec-command`) are rejected unless the server was started
with `--allow-shell`. Commands must start with `-`. See
`framewalk://reference/allowed-mi` for the canonical allowlist.

### `(mi-quote string)` → string

Apply MI c-string quoting to a parameter value. Returns the string
unchanged if it contains no special characters, or wraps it in a
c-string literal with ISO C escapes.

```scheme
(mi-quote "main")                   ;; => "main"
(mi-quote "/path/with spaces/file") ;; => "\"/path/with spaces/file\""
(mi-quote "x + \"y\"")             ;; => "\"x + \\\"y\\\"\""
```

This is the building block for `mi-cmd` (defined in the prelude).
Use it when constructing raw `(mi ...)` calls with dynamic arguments
to prevent parameter injection from paths with spaces, expressions
with quotes, etc.

### `(gdb-version)` → result-entry-list

Query GDB's banner text in a Scheme-friendly shape.

```scheme
(result-field "version" (gdb-version))
;; => "GNU gdb (GDB) 15.1"
```

Unlike raw `(mi "-gdb-version")`, this helper captures the console
banner and returns it as a synthetic `"version"` field so callers do not
need a separate `drain-events` step just to learn the debugger version.

### `(wait-for-stop)` → hash-map

Return the current stop immediately if the target is already halted;
otherwise block until GDB reports the next `*stopped` event (target hit
a breakpoint, received a signal, finished a step, etc.).

```scheme
(run)
(wait-for-stop)   ;; blocks until *stopped arrives
;; => hash-map with "reason", "thread", "raw" keys
```

**Returns** a hash-map with:
- `"reason"` — stop reason as a string (e.g. `"breakpoint-hit"`, `"signal-received"`)
- `"thread"` — thread ID as a string
- `"raw"` — lossless result-entry list of all raw MI fields from the stop record

**Timeout:** uses the server default wait timeout unless you override it
per call:

```scheme
(wait-for-stop)              ;; use server default
(wait-for-stop 300)          ;; positional override in seconds
(wait-for-stop timeout: 300) ;; tagged override in seconds
```

Timeout failures include the locally observed target state and last-seen
async record so you can tell the difference between "still running",
"already exited", and "transport died".

## Result format

MI results are returned as lossless result-entry lists. Each element is
a tiny hash-map with `"name"` and `"value"` keys:

```scheme
;; A typical breakpoint result:
(define bp (set-breakpoint "main"))
;; bp is a list like:
;; ({"name" => "bkpt", "value" => (...entry list...)})

;; Access unique fields with result-field:
(define bkpt (result-field "bkpt" bp))
(result-field "number" bkpt)  ;; => "1"

;; Access repeated fields with result-fields:
(define stack (result-field "stack" (mi "-stack-list-frames")))
(define frames (result-fields "frame" stack))
```

MI value types map to Scheme types:

| MI type | Scheme type |
|---|---|
| const `"string"` | string |
| tuple `{key=val,...}` | result-entry list |
| list `[val,...]` | list |
| list `[key=val,...]` | result-entry list |
| empty list `[]` | `'()` |

## Prelude functions

These are defined in Scheme (loaded at engine startup) and wrap
the `(mi ...)` primitive for convenience. All prelude functions use
`mi-cmd` internally to ensure proper parameter quoting.

### Safe command builder

```scheme
(mi-cmd operation param ...)
```

Build and submit an MI command with all parameters properly quoted via
`mi-quote`. `operation` is the MI operation name without the leading `-`.

```scheme
(mi-cmd "file-exec-and-symbols" "/path/with spaces/a.out")
;; sends: -file-exec-and-symbols "/path/with spaces/a.out"

(mi-cmd "data-evaluate-expression" "x + 1")
;; sends: -data-evaluate-expression "x + 1"

(mi-cmd "exec-run")
;; sends: -exec-run
```

Prefer `mi-cmd` over raw `(mi ...)` when building commands with
dynamic arguments to avoid injection from paths with spaces or
expressions with quotes.

### Session

| Function | MI command | Description |
|---|---|---|
| `(gdb-version)` | `-gdb-version` + console banner capture | Query GDB version |
| `(load-file path)` | `-file-exec-and-symbols` | Load executable + symbols |
| `(attach pid)` | `-target-attach` | Attach to running process (`pid` may be a string or number) |
| `(detach)` | `-target-detach` | Detach from target |

### Execution control

| Function | MI command | Description |
|---|---|---|
| `(run)` | `-exec-run` | Start program from beginning |
| `(cont)` | `-exec-continue` | Continue from current stop |
| `(step)` | `-exec-step` | Step into (source line) |
| `(next)` | `-exec-next` | Step over (source line) |
| `(finish)` | `-exec-finish` | Run until current function returns |
| `(interrupt)` | `-exec-interrupt --all` | Pause all running target threads |
| `(until loc)` | `-exec-until` | Run to location or next line |
| `(step-instruction)` | `-exec-step-instruction` | Step into (machine instruction) |
| `(next-instruction)` | `-exec-next-instruction` | Step over (machine instruction) |
| `(reverse-step)` | `-exec-step --reverse` | Step backward one source line |
| `(reverse-next)` | `-exec-next --reverse` | Step-over backward one source line |
| `(reverse-continue)` | `-exec-continue --reverse` | Continue backward |
| `(reverse-finish)` | `-exec-finish --reverse` | Run backward to caller |

All execution commands above that put the target into the running
state return immediately on GDB's `^running` acknowledgement. The
eventual `*stopped` event (hit breakpoint, signal, end of step, …)
arrives asynchronously — your Scheme code will NOT see it as a
return value.

**To wait for the next stop caused by a command, use the `*-and-wait`
primitives below.** They are the recommended way to issue any
execution command because they capture the transport's event cursor
before submitting the MI command, then wait for the first stop
strictly after that point (see [Caveats](#caveats-on-wait-for-stop)).

Reverse commands require GDB's reverse-debugging support (e.g.
`target record-full`).

### Execution + stop (atomic trigger-and-wait)

| Function | MI command | Description |
|---|---|---|
| `(run-and-wait)` | `-exec-run` | Start program, return the next `*stopped` event |
| `(cont-and-wait)` | `-exec-continue` | Continue, return the next `*stopped` event |
| `(step-and-wait)` | `-exec-step` | Step into, return the resulting stop |
| `(next-and-wait)` | `-exec-next` | Step over, return the resulting stop |
| `(finish-and-wait)` | `-exec-finish` | Run until the current function returns |
| `(until-and-wait loc)` | `-exec-until` | Run to a location or the next source line |

Each primitive captures the transport's event cursor **before**
submitting the MI command, then waits for the first `*stopped`
observed strictly after that cursor. They return the same hash-map
shape as `(wait-for-stop)` (`"reason"`, `"thread"`, `"raw"`). Each
uses the server's default wait timeout, with per-call overrides in both
positional and tagged forms:

```scheme
(cont-and-wait)               ;; default timeout
(cont-and-wait 300)           ;; positional override
(cont-and-wait timeout: 300) ;; tagged override
(until-and-wait "panic" 120)
(until-and-wait "panic" timeout: 120)
```

If the target does not stop within that window, the primitive raises a
Scheme error with target-state context from framewalk's local event
journal.

If the command does not transition the target into the running state
(for example, `-exec-continue` on an already-exited target returns
`^error`), the primitive returns the outcome directly rather than
waiting for a stop that will never arrive.

#### Caveats on `wait-for-stop`

The standalone `(wait-for-stop)` primitive is now journal-backed, so
it no longer misses a stop that has already happened by the time the
call begins. But it still does **not** correlate a stop with the
command you issued just before it. If you write:

```scheme
(run)
(wait-for-stop)
```

then `wait-for-stop` means "current stop if already stopped, otherwise
next stop from now," not "the stop caused by this exact `run`." That
is why the `*-and-wait` primitives remain the right interface for any
trigger-then-wait flow: they capture the cursor before the trigger and
wait only for later stops. Use standalone `wait-for-stop` when the stop
is caused by something external to your next MI command.

### Breakpoints

| Function | MI command | Description |
|---|---|---|
| `(set-breakpoint loc)` | `-break-insert` | Insert software breakpoint |
| `(set-temp-breakpoint loc)` | `-break-insert -t` | Insert temporary software breakpoint (auto-deleted on hit) |
| `(set-hw-breakpoint loc)` | `-break-insert -h` | Insert hardware breakpoint (CPU debug register) |
| `(set-temp-hw-breakpoint loc)` | `-break-insert -t -h` | Insert temporary hardware breakpoint |
| `(delete-breakpoint id)` | `-break-delete` | Delete breakpoint by string or number id |
| `(enable-breakpoint id)` | `-break-enable` | Re-enable a disabled breakpoint by string or number id |
| `(disable-breakpoint id)` | `-break-disable` | Disable without deleting by string or number id |
| `(list-breakpoints)` | `-break-list` | List all breakpoints |

`loc` is a GDB location string: `"main"`, `"file.c:42"`, `"*0x400520"`.

### Stack inspection

| Function | MI command | Description |
|---|---|---|
| `(backtrace)` | `-stack-list-frames` | Call stack as a list of frame values |
| `(list-locals)` | `-stack-list-locals 1` | Local variables in current frame |
| `(list-arguments)` | `-stack-list-arguments 1` | Function arguments |
| `(stack-depth)` | `-stack-info-depth` | Number of frames |
| `(select-frame n)` | `-stack-select-frame` | Select frame by level number or numeric string |

### Threads

| Function | MI command | Description |
|---|---|---|
| `(list-threads)` | `-thread-info` | All threads with state |
| `(select-thread id)` | `-thread-select` | Switch to thread by string or number ID |

The `list_threads` MCP tool (and its underlying `-thread-info` command)
also accepts an optional `thread_id` parameter to query a single
thread; from Scheme, pass it directly via
`(mi-cmd "thread-info" "1")`.

### Variables

| Function | MI command | Description |
|---|---|---|
| `(inspect expr)` | `-data-evaluate-expression` | Evaluate expression in current frame |

`expr` is a GDB expression: `"argc"`, `"arr[3]"`, `"ptr->field"`,
`"sizeof(int)"`. The expression is quoted automatically.

### Event journal

| Function | Description |
|---|---|
| `(drain-events)` | Return all retained events as a list of hash-maps |
| `(drain-events n)` / `(drain-events after: n)` | Return events after sequence `n` |

Each event hash-map has `"seq"` (integer), `"kind"` (string: `"log"`,
`"console"`, `"stopped"`, `"running"`, `"notify"`, `"status"`,
`"target-output"`, `"parse-error"`), and optional `"text"`, `"class"`,
`"thread"`, `"reason"` fields.

Use `drain-events` after commands to inspect GDB warnings (e.g. failed
SW breakpoint installs), console output, and async notifications from
within Scheme. This is the Scheme-side equivalent of the MCP
`drain_events` tool.

### Result accessors

```scheme
(result-field name result)
(result-fields name result)
```

`result-field` extracts a uniquely named value from a result-entry list.
It returns `#f` when absent and raises if multiple values exist for the
same field name.

`result-fields` extracts all matching values from a result-entry list,
preserving MI order. Use it for repeated keys such as `frame=...`.

### Composition helpers

```scheme
(step-n n)
```
Step into `n` times, **waiting for each stop before issuing the
next step**, and return a list of the `n` resulting stop hash-maps.
Uses `step-and-wait` internally because MI execution commands return
on `^running` and the `*stopped` event arrives asynchronously —
issuing another `-exec-step` before the target has stopped would be
rejected by GDB with `^error`.

```scheme
(next-n n)
```
Step-over `n` times, with the same stop-awaiting semantics as
`step-n`.

```scheme
(run-to loc)
```
Set a temporary breakpoint at `loc`, then call `(run-and-wait)`.
Returns the stopped-event hash-map. Internally this uses
`run-and-wait` (not a separate `(run)` + `(wait-for-stop)` pair) so
it stays correlated to the specific `run` command as described under
[Caveats on `wait-for-stop`](#caveats-on-wait-for-stop).

## Patterns

### Collect data across stops

```scheme
(define (collect-at-stops n)
  (let loop ((i 0) (acc '()))
    (if (>= i n) (reverse acc)
        (begin
          (cont-and-wait)
          (loop (+ i 1)
                (cons (list-locals) acc))))))
```

`cont-and-wait` is used rather than `(cont)` + `(wait-for-stop)` to
keep the stop correlated to that specific continue command — see
[Caveats on `wait-for-stop`](#caveats-on-wait-for-stop).

### Build reusable helpers

State persists, so define helpers in one call and use them in the next:

```scheme
;; Call 1: define helper
(define (snapshot loc)
  (set-temp-breakpoint loc)
  (run-and-wait)
  (list (backtrace) (list-locals)))

;; Call 2: use it
(snapshot "process_request")
```

### Walk a linked list

```scheme
(define (walk-list ptr-expr max)
  (let loop ((i 0) (cur ptr-expr) (acc '()))
    (if (>= i max) (reverse acc)
        (let ((val (inspect (string-append cur "->value")))
              (nxt (inspect (string-append cur "->next"))))
          (if (equal? (result-field "value" nxt) "0x0")
              (reverse (cons val acc))
              (loop (+ i 1)
                    (string-append "(" cur "->next)")
                    (cons val acc)))))))
```

### Count events

```scheme
(define alloc-count 0)
(define free-count 0)

(define (tally-allocs n)
  (let loop ((i 0))
    (if (>= i n) (list "allocs" alloc-count "frees" free-count)
        (begin
          (wait-for-stop)
          (let* ((stack (result-field "stack" (backtrace)))
                 (top-frame (car (result-fields "frame" stack)))
                 (func (result-field "func" top-frame)))
            (cond
              ((equal? func "my_alloc") (set! alloc-count (+ alloc-count 1)))
              ((equal? func "my_free")  (set! free-count (+ free-count 1))))
            (cont)
            (loop (+ i 1)))))))
```

### Error handling

```scheme
(with-handler
  (lambda (err)
    (displayln (format "caught: ~a" err))
    'error)
  (inspect "nonexistent_variable"))
```

Use `with-handler` around loops or sampling helpers when you want to
keep partial results instead of aborting the whole `scheme_eval` block
on the first late GDB error.

## Direct MI access

The prelude covers common operations. For anything else, use
`(mi-cmd ...)` for commands with dynamic parameters:

```scheme
(mi-cmd "data-read-memory-bytes" "0x7fff00" "32")
(mi-cmd "var-create" "-" "*" "some_expression")
```

Or use `(mi ...)` directly for commands with MI options (flags like
`-c`, `--no-frame-filters`) where `mi-cmd` would not apply the
correct option syntax:

```scheme
(mi "-stack-list-frames --no-frame-filters 0 5")
(mi (string-append "-break-insert -c " (mi-quote "x > 10") " file.c:42"))
```

Refer to the [GDB/MI documentation](https://sourceware.org/gdb/current/onlinedocs/gdb.html/GDB_002fMI.html)
for the full command set.
