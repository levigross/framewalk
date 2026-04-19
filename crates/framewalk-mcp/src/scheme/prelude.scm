;;; framewalk scheme prelude
;;;
;;; Convenience wrappers over the `mi`, `mi-quote`, and `wait-for-stop`
;;; Rust primitives.  Loaded into the Steel engine at startup via
;;; `include_str!`.  All definitions here are available to every
;;; `scheme_eval` call.

;; ----------------------------------------------------------------
;; Safe MI command builder
;; ----------------------------------------------------------------

;; `mi-cmd` is a `syntax-rules` macro, not a variadic function.
;;
;; WHY A MACRO: Steel 0.8.2's sandboxed engine has a compiler bug that
;; trips on variadic rest-arg functions when the rest-arg is consumed
;; inside the body and the call site passes zero variadic arguments.
;; Symptom: `FreeIdentifier: Cannot reference an identifier before its
;; definition: ##params2`, where `##params2` is Steel's mangled internal
;; name for the rest-arg.  None of the runtime workarounds (lifted
;; lambda, let-rebound, recursive helper) bypass it — every variant
;; that uses `(define (mi-cmd op . params) ...)` fails on an empty
;; variadic call like `(mi-cmd "gdb-version")`.
;;
;; A `syntax-rules` macro sidesteps the whole class of bugs: expansion
;; happens at parse time, before the interpreter ever sees a rest-arg.
;; Every call site becomes a literal `string-append` chain.  Do NOT
;; convert this back to a variadic function without first confirming
;; the Steel bug is fixed upstream and the `prelude_bootstrap_smoke`
;; unit test still passes.
;;
;; Example expansions:
;;   (mi-cmd "gdb-version")
;;     => (mi (string-append "-" "gdb-version"))
;;   (mi-cmd "file-exec-and-symbols" "/path/with spaces/a.out")
;;     => (mi (string-append "-" "file-exec-and-symbols"
;;                           (string-append " " (mi-quote "/path/with spaces/a.out"))))
(define-syntax mi-args
  (syntax-rules ()
    ((_)                "")
    ((_ arg)            (string-append " " (mi-quote arg)))
    ((_ arg rest ...)   (string-append " " (mi-quote arg) (mi-args rest ...)))))

(define-syntax mi-cmd
  (syntax-rules ()
    ((_ operation)
     (mi (string-append "-" operation)))
    ((_ operation arg ...)
     (mi (string-append "-" operation (mi-args arg ...))))))

;; ----------------------------------------------------------------
;; Session
;; ----------------------------------------------------------------

(define (load-file path)
  (mi-cmd "file-exec-and-symbols" path))

(define (id->string who value)
  (cond
    ((string? value) value)
    ((number? value) (number->string value))
    (else
      (error (string-append
               who
               " expects an id as a string or number")))))

(define (attach pid)
  (mi-cmd "target-attach" (id->string "attach" pid)))

(define (detach)
  (mi-cmd "target-detach"))

;; ----------------------------------------------------------------
;; Execution control
;; ----------------------------------------------------------------

(define (run)
  (mi-cmd "exec-run"))

(define (cont)
  (mi-cmd "exec-continue"))

(define (step)
  (mi-cmd "exec-step"))

(define (next)
  (mi-cmd "exec-next"))

(define (finish)
  (mi-cmd "exec-finish"))

(define (interrupt)
  (mi "-exec-interrupt --all"))

(define (until loc)
  (mi-cmd "exec-until" loc))

(define (step-instruction)
  (mi-cmd "exec-step-instruction"))

(define (next-instruction)
  (mi-cmd "exec-next-instruction"))

;; Reverse execution — requires `target record-full` or equivalent.
(define (reverse-step)
  (mi "-exec-step --reverse"))

(define (reverse-next)
  (mi "-exec-next --reverse"))

(define (reverse-continue)
  (mi "-exec-continue --reverse"))

(define (reverse-finish)
  (mi "-exec-finish --reverse"))

;; ----------------------------------------------------------------
;; Breakpoints
;; ----------------------------------------------------------------

(define (set-breakpoint loc)
  (mi-cmd "break-insert" loc))

(define (set-temp-breakpoint loc)
  ;; The `-t` flag is a literal option, not user input; only `loc`
  ;; needs quoting. We use raw `mi` here because `mi-cmd` treats all
  ;; arguments as positional parameters.
  (mi (string-append "-break-insert -t " (mi-quote loc))))

(define (set-hw-breakpoint loc)
  ;; Hardware breakpoint — uses a debug register instead of a software
  ;; INT3 patch.  Required for early-boot kernel addresses that aren't
  ;; paged in yet, and for read-only memory regions.
  (mi (string-append "-break-insert -h " (mi-quote loc))))

(define (set-temp-hw-breakpoint loc)
  ;; Temporary hardware breakpoint — auto-deleted after the first hit.
  (mi (string-append "-break-insert -t -h " (mi-quote loc))))

(define (delete-breakpoint id)
  (mi-cmd "break-delete" (id->string "delete-breakpoint" id)))

(define (enable-breakpoint id)
  (mi-cmd "break-enable" (id->string "enable-breakpoint" id)))

(define (disable-breakpoint id)
  (mi-cmd "break-disable" (id->string "disable-breakpoint" id)))

(define (list-breakpoints)
  (mi-cmd "break-list"))

;; ----------------------------------------------------------------
;; Stack inspection
;; ----------------------------------------------------------------

;; `backtrace` is a macro so callers can write `(backtrace)` for the
;; full stack or `(backtrace limit: 5)` / `(backtrace 5)` to cap the
;; frame count.  MI's `-stack-list-frames` takes inclusive `low high`
;; positional arguments, so `limit: N` expands to `low=0 high=N-1`.
(define-syntax backtrace
  (syntax-rules (limit:)
    ((backtrace)              (backtrace/all))
    ((backtrace limit: n)     (backtrace/limit n))
    ((backtrace n)            (backtrace/limit n))))

(define (backtrace/all)
  (let ((stack (result-field "stack" (mi-cmd "stack-list-frames"))))
    (if stack
        (result-fields "frame" stack)
        '())))

(define (backtrace/limit n)
  (let ((stack (result-field "stack"
                 (mi-cmd "stack-list-frames"
                         "0"
                         (number->string (- n 1))))))
    (if stack
        (result-fields "frame" stack)
        '())))

(define (list-locals)
  (mi "-stack-list-locals 1"))

(define (list-arguments)
  (mi "-stack-list-arguments 1"))

(define (stack-depth)
  (mi-cmd "stack-info-depth"))

(define (select-frame level)
  (mi-cmd "stack-select-frame" (id->string "select-frame" level)))

;; ----------------------------------------------------------------
;; Threads
;; ----------------------------------------------------------------

(define (list-threads)
  (mi-cmd "thread-info"))

(define (select-thread id)
  (mi-cmd "thread-select" (id->string "select-thread" id)))

;; ----------------------------------------------------------------
;; Variables and expressions
;; ----------------------------------------------------------------

(define (inspect expr)
  (mi-cmd "data-evaluate-expression" expr))

;; ----------------------------------------------------------------
;; Stop-wait wrappers with optional timeout override
;; ----------------------------------------------------------------

(define-syntax wait-for-stop
  (syntax-rules (timeout:)
    ((wait-for-stop)
     (wait-for-stop/default))
    ((wait-for-stop timeout: seconds)
     (wait-for-stop/timeout seconds))
    ((wait-for-stop seconds)
     (wait-for-stop/timeout seconds))))

(define-syntax run-and-wait
  (syntax-rules (timeout:)
    ((run-and-wait)
     (run-and-wait/default))
    ((run-and-wait timeout: seconds)
     (run-and-wait/timeout seconds))
    ((run-and-wait seconds)
     (run-and-wait/timeout seconds))))

(define-syntax cont-and-wait
  (syntax-rules (timeout:)
    ((cont-and-wait)
     (cont-and-wait/default))
    ((cont-and-wait timeout: seconds)
     (cont-and-wait/timeout seconds))
    ((cont-and-wait seconds)
     (cont-and-wait/timeout seconds))))

(define-syntax step-and-wait
  (syntax-rules (timeout:)
    ((step-and-wait)
     (step-and-wait/default))
    ((step-and-wait timeout: seconds)
     (step-and-wait/timeout seconds))
    ((step-and-wait seconds)
     (step-and-wait/timeout seconds))))

(define-syntax next-and-wait
  (syntax-rules (timeout:)
    ((next-and-wait)
     (next-and-wait/default))
    ((next-and-wait timeout: seconds)
     (next-and-wait/timeout seconds))
    ((next-and-wait seconds)
     (next-and-wait/timeout seconds))))

(define-syntax finish-and-wait
  (syntax-rules (timeout:)
    ((finish-and-wait)
     (finish-and-wait/default))
    ((finish-and-wait timeout: seconds)
     (finish-and-wait/timeout seconds))
    ((finish-and-wait seconds)
     (finish-and-wait/timeout seconds))))

(define-syntax until-and-wait
  (syntax-rules (timeout:)
    ((until-and-wait loc)
     (until-and-wait/default loc))
    ((until-and-wait loc timeout: seconds)
     (until-and-wait/timeout loc seconds))
    ((until-and-wait loc seconds)
     (until-and-wait/timeout loc seconds))))

;; ----------------------------------------------------------------
;; Event journal
;; ----------------------------------------------------------------

(define-syntax drain-events
  (syntax-rules (after:)
    ((drain-events)
     (drain-events/all))
    ((drain-events after: seq)
     (drain-events/after seq))
    ((drain-events seq)
     (drain-events/after seq))))

;; ----------------------------------------------------------------
;; Result accessors
;; ----------------------------------------------------------------

(define (result-fields name result)
  "Extract all matching values from a result-entry list, preserving
   order. Returns the empty list when the field is absent."
  (if (not result)
      '()
      (let loop ((xs result) (acc '()))
        (cond
          ((null? xs) (reverse acc))
          ((equal? (hash-ref (car xs) "name") name)
           (loop (cdr xs)
                 (cons (hash-ref (car xs) "value") acc)))
          (else
           (loop (cdr xs) acc))))))

(define (result-field name result)
  "Extract a uniquely named field from a result-entry list. Returns #f
   when the field is absent. Raises if multiple values exist; use
   `result-fields` for repeated MI keys."
  (let ((matches (result-fields name result)))
    (cond
      ((null? matches) #f)
      ((null? (cdr matches)) (car matches))
      (else
       (error (string-append
                "result-field expected one value for "
                name
                "; use result-fields for repeated MI keys"))))))

;; ----------------------------------------------------------------
;; Compact result rendering
;; ----------------------------------------------------------------

;; `(compact x)` collapses a lossless result-entry list into a flat
;; hash-map when every entry's "name" is unique.  When names repeat
;; (e.g. `-stack-list-frames` returns multiple `frame` entries) the
;; list is preserved so no information is lost.  Non-list values pass
;; through unchanged, so `(compact x)` is always safe to wrap around
;; an MI result.  Each entry's "value" is recursively compacted, so
;; `(compact (backtrace limit: 4))` leaves the outer 4-frame list
;; intact while collapsing each frame's inner tuple to a flat map.
;;
;; Example:
;;   (compact (list (hash "name" "a" "value" "1")
;;                  (hash "name" "b" "value" "2")))
;;     => (hash "a" "1" "b" "2")

(define (compact x)
  (cond
    ((not (list? x)) x)
    ((null? x) (hash))
    ((compact/all-entries? x)
     (let ((pairs (compact/entry-pairs x)))
       (if (compact/unique-names? pairs '())
           (compact/pairs->hash pairs (hash))
           (compact/rebuild-entries x))))
    (else (map compact x))))

(define (compact/entry? e)
  (and (hash? e)
       (= 2 (hash-length e))
       (hash-contains? e "name")
       (hash-contains? e "value")))

(define (compact/all-entries? xs)
  (cond
    ((null? xs) #f)
    ((null? (cdr xs)) (compact/entry? (car xs)))
    (else (and (compact/entry? (car xs))
               (compact/all-entries? (cdr xs))))))

(define (compact/entry-pairs xs)
  (if (null? xs)
      '()
      (cons (cons (hash-ref (car xs) "name")
                  (compact (hash-ref (car xs) "value")))
            (compact/entry-pairs (cdr xs)))))

(define (compact/unique-names? pairs seen)
  (cond
    ((null? pairs) #t)
    ((compact/seen? (car (car pairs)) seen) #f)
    (else (compact/unique-names? (cdr pairs)
                                 (cons (car (car pairs)) seen)))))

(define (compact/seen? needle haystack)
  (cond
    ((null? haystack) #f)
    ((equal? needle (car haystack)) #t)
    (else (compact/seen? needle (cdr haystack)))))

(define (compact/pairs->hash pairs acc)
  (if (null? pairs)
      acc
      (compact/pairs->hash (cdr pairs)
                           (hash-insert acc
                                        (car (car pairs))
                                        (cdr (car pairs))))))

(define (compact/rebuild-entries xs)
  (if (null? xs)
      '()
      (cons (hash-insert (hash-insert (hash)
                                      "name" (hash-ref (car xs) "name"))
                         "value" (compact (hash-ref (car xs) "value")))
            (compact/rebuild-entries (cdr xs)))))

;; ----------------------------------------------------------------
;; Composition helpers
;; ----------------------------------------------------------------

(define (step-n n)
  "Step `n` times, waiting for a stop between each step, and
  collecting the resulting `*stopped` hash-maps into a list.

  Uses `step-and-wait` rather than raw `(step)` — execution commands
  return on `^running` and the `*stopped` record arrives
  asynchronously, so a loop of bare `(step)` calls would issue each
  step while the target was still running from the previous one.
  GDB rejects overlapped execution commands with a
  `^error,msg=\"Cannot execute ... while the target is running.\"`,
  so the old form returned a list of errors that looked superficially
  like step results."
  (let loop ((i 0) (results '()))
    (if (>= i n)
        (reverse results)
        (loop (+ i 1) (cons (step-and-wait) results)))))

(define (next-n n)
  "Step-over `n` times, waiting for a stop between each step, and
  collecting the resulting `*stopped` hash-maps into a list.

  See the note on `step-n` for why this uses `next-and-wait` instead
  of `(next)`."
  (let loop ((i 0) (results '()))
    (if (>= i n)
        (reverse results)
        (loop (+ i 1) (cons (next-and-wait) results)))))

(define (run-to loc)
  "Set a temporary breakpoint at `loc`, run, and wait for the stop.

  Uses `run-and-wait` internally rather than `(run)` followed by
  `(wait-for-stop)` — the two-step form is not correlated to the
  specific `run` command that triggered it. `run-and-wait` captures
  the event cursor before running, then waits only for later stops."
  (set-temp-breakpoint loc)
  (run-and-wait))
