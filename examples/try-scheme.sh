#!/usr/bin/env bash
# Drive framewalk-mcp in scheme mode from the command line.
#
# Usage:
#   # First, compile the test program:
#   gcc -g -O0 -o /tmp/hello examples/hello.c
#
#   # Then run this script:
#   nix develop --command bash examples/try-scheme.sh /tmp/hello
#
# Each example spawns a fresh framewalk-mcp process, sends MCP
# JSON-RPC over stdin, and prints the scheme_eval result.

set -euo pipefail

BINARY="${1:?Usage: $0 <path-to-debug-binary>}"
BINARY="$(realpath "$BINARY")"

# Build framewalk-mcp if needed.
cargo build -p framewalk-mcp --quiet 2>/dev/null || true
FW="$(cargo build -p framewalk-mcp --message-format=json --quiet 2>/dev/null \
    | jq -r 'select(.executable != null) | .executable' | head -1)"
if [ -z "$FW" ]; then
    FW="target/debug/framewalk-mcp"
fi

echo "==> Using framewalk-mcp: $FW"
echo "==> Debug target: $BINARY"
echo

# Helper: send JSON-RPC messages and print the scheme_eval reply.
run_session() {
    local scheme_code="$1"
    local desc="$2"

    echo "--- $desc ---"
    echo "Code: $scheme_code"
    echo

    local escaped
    escaped="$(printf '%s' "$scheme_code" | jq -Rs .)"

    {
        printf '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"try-scheme","version":"0"}}}\n'
        printf '{"jsonrpc":"2.0","method":"notifications/initialized"}\n'
        printf '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"scheme_eval","arguments":{"code":%s}}}\n' "$escaped"
        sleep 4
    } | "$FW" --mode scheme 2>/dev/null | while IFS= read -r line; do
        id="$(echo "$line" | jq -r '.id // empty' 2>/dev/null)"
        if [ "$id" = "2" ]; then
            is_err="$(echo "$line" | jq -r '.result.isError // false')"
            text="$(echo "$line" | jq -r '.result.content[0].text // .error.message // "no output"')"
            if [ "$is_err" = "true" ]; then
                echo "ERROR: $text"
            else
                echo "Result: $text"
            fi
        fi
    done
    echo
}

# =====================================================================
#  Pure Scheme
# =====================================================================

run_session \
    "(+ 1 2 3 4 5)" \
    "Pure arithmetic"

run_session \
    "(map (lambda (x) (* x x)) '(1 2 3 4 5))" \
    "List operations — squares"

run_session \
    "(define (fib n) (if (<= n 1) n (+ (fib (- n 1)) (fib (- n 2)))))
     (map fib '(0 1 2 3 4 5 6 7 8 9 10))" \
    "Fibonacci sequence computed in Scheme"

# =====================================================================
#  GDB basics
# =====================================================================

run_session \
    "(gdb-version)" \
    "GDB version via prelude"

run_session \
    "(mi \"-list-features\")" \
    "Raw MI: list GDB features"

# =====================================================================
#  Load, breakpoint, run, inspect
# =====================================================================

run_session \
    "(begin (load-file \"$BINARY\") (set-breakpoint \"main\") (run) (wait-for-stop) (backtrace))" \
    "Load binary, break at main, backtrace"

run_session \
    "(begin (load-file \"$BINARY\") (run-to \"main\") (inspect \"argc\"))" \
    "Run to main, inspect argc"

# =====================================================================
#  Struct inspection
# =====================================================================

run_session \
    "(begin
       (load-file \"$BINARY\")
       (set-breakpoint \"distance\")
       (run)
       (wait-for-stop)
       (list (inspect \"a.x\") (inspect \"a.y\") (inspect \"b.x\") (inspect \"b.y\")))" \
    "Break at distance(), inspect Point struct fields"

# =====================================================================
#  Linked list traversal
# =====================================================================

run_session \
    "(begin
       (load-file \"$BINARY\")
       (set-breakpoint \"list_sum\")
       (run)
       (wait-for-stop)
       (list (inspect \"head->value\")
             (inspect \"head->next->value\")
             (inspect \"head->next->next->value\")))" \
    "Break at list_sum, follow linked list pointers"

# =====================================================================
#  Array inspection — break inside bubble_sort
# =====================================================================

run_session \
    "(begin
       (load-file \"$BINARY\")
       (set-breakpoint \"bubble_sort\")
       (run)
       (wait-for-stop)
       (inspect \"len\"))" \
    "Break at bubble_sort, inspect array length"

# =====================================================================
#  Step through code collecting locals
# =====================================================================

run_session \
    "(begin
       (load-file \"$BINARY\")
       (set-breakpoint \"sum_array\")
       (run)
       (wait-for-stop)
       (list-locals))" \
    "Break at sum_array, show all locals"

# =====================================================================
#  Multi-step: step-n and collect
# =====================================================================

run_session \
    "(begin
       (load-file \"$BINARY\")
       (run-to \"main\")
       (length (step-n 5)))" \
    "Run to main, step 5 times (returns count)"

# =====================================================================
#  Recursive call tracking — factorial
# =====================================================================

run_session \
    "(begin
       (load-file \"$BINARY\")
       (set-breakpoint \"factorial\")
       (run)
       (wait-for-stop)
       (define (collect-n-stops n)
         (let loop ((i 0) (acc '()))
           (if (>= i n) (reverse acc)
               (begin
                 (cont)
                 (wait-for-stop)
                 (loop (+ i 1) (cons (inspect \"n\") acc))))))
       (collect-n-stops 5))" \
    "Break at factorial, collect n across 5 recursive calls"

# =====================================================================
#  String inspection
# =====================================================================

run_session \
    "(begin
       (load-file \"$BINARY\")
       (set-breakpoint \"reverse_string\")
       (run)
       (wait-for-stop)
       (define before (inspect \"s\"))
       (finish)
       (define after (inspect \"s\"))
       (list before after))" \
    "Break at reverse_string, capture before/after"

# =====================================================================
#  Custom helper persists across calls (state demo)
# =====================================================================

run_session \
    "(begin
       ;; Define a reusable helper right in Scheme
       (define (breakpoint-snapshot loc)
         \"Set a temporary breakpoint, run to it, and grab locals.\"
         (set-temp-breakpoint loc)
         (run)
         (wait-for-stop)
         (list-locals))

       (load-file \"$BINARY\")
       (breakpoint-snapshot \"sum_array\"))" \
    "Define reusable breakpoint-snapshot helper, use it"

# =====================================================================
#  Security: shell-adjacent MI commands are blocked
# =====================================================================

run_session \
    "(mi \"-interpreter-exec console \\\"shell echo pwned\\\"\")" \
    "Security: shell pivot rejected (should show ERROR)"

echo "==> Done."
