# Scheme Quick Reference

Cheat sheet for `scheme_eval` sessions. See
[docs/scheme-reference.md](../docs/scheme-reference.md) for the full
reference.

## Minimal session

```scheme
(begin
  (load-file "/path/to/binary")
  (set-breakpoint "main")
  (run)
  (wait-for-stop)
  (backtrace))
```

## Available functions

```
Session:     (gdb-version) (load-file p) (attach pid) (detach)
Execution:   (run) (cont) (step) (next) (finish) (interrupt) (until loc)
Breakpoints: (set-breakpoint loc) (set-temp-breakpoint loc)
             (delete-breakpoint id) (list-breakpoints)
Stack:       (backtrace) (list-locals) (list-arguments)
             (stack-depth) (select-frame n)
Threads:     (list-threads) (select-thread id)
Variables:   (inspect expr)
Helpers:     (step-n n) (next-n n) (run-to loc)
             (result-field key result) (result-fields key result)
Raw MI:      (mi "-any-mi-command args...")
Wait:        (wait-for-stop)
```

## Result access

```scheme
;; Results are lossless result-entry lists:
(define bp (set-breakpoint "main"))
(define bkpt (result-field "bkpt" bp))
(result-field "number" bkpt)
```

## State persists

```scheme
;; Call 1:
(define (snap loc)
  (set-temp-breakpoint loc) (run) (wait-for-stop) (list-locals))

;; Call 2 (snap is still defined):
(snap "process_data")
```

## Execution pattern

```scheme
;; run/cont/step return 'running immediately.
;; Always follow with (wait-for-stop):
(run)
(wait-for-stop)  ;; blocks until target halts

;; Or use the shorthand:
(run-to "main")  ;; sets temp bp, runs, waits
```

## Collect data in a loop

```scheme
(define (collect n)
  (let loop ((i 0) (acc '()))
    (if (>= i n) (reverse acc)
        (begin
          (step-and-wait)
          (loop (+ i 1)
                (cons (inspect "my_var") acc))))))

(collect 10)  ;; => list of 10 snapshots
```

## Keep Partial Results

```scheme
(with-handler
  (lambda (err) 'stopped-early)
  (inspect "possibly_missing_value"))
```
