# Recipe: debug an early-boot kernel under QEMU

## Goal

Connect framewalk to a QEMU-hosted kernel via its GDB stub, set a
hardware breakpoint at `start_kernel` (or any early-boot function), and
inspect state before page tables are fully initialized.

## Prerequisites

- A debug-enabled kernel (e.g. `nix build .#kernel-debug-vm`)
- QEMU started with `-s -S` (GDB stub on `:1234`, CPU paused at reset)
- framewalk-mcp started with `--no-non-stop` (QEMU's gdbstub is all-stop only)

## Why `--no-non-stop`?

framewalk defaults to GDB non-stop mode (`-gdb-set non-stop on`),
which allows individual threads to stop while others continue.  QEMU's
gdbstub does not support non-stop and rejects the connection with
"Non-stop mode requested, but remote does not support non-stop."
Pass `--no-non-stop` or set `FRAMEWALK_NON_STOP=false` to skip the
bootstrap command.

## Why hardware breakpoints?

Early-boot kernel code runs before the page tables that map the
kernel's `.text` section are active.  GDB's default software
breakpoints work by patching an `INT3` instruction into memory at the
target address.  If the page isn't mapped yet, the write silently fails
(GDB emits `&"warning: ..."` on the MI log channel) and the subsequent
continue appears to work but never actually stops.

Hardware breakpoints (`-break-insert -h`) use CPU debug registers,
which trigger regardless of whether the address is paged in.  Use
`(set-hw-breakpoint loc)` or `(set-temp-hw-breakpoint loc)` for any
address that might not be mapped.

## Steps (Scheme mode)

```scheme
;; 1. Connect to QEMU's GDB stub
(mi "-target-select remote localhost:1234")

;; 2. Load kernel symbols (adjust path to your vmlinux)
(mi "-file-symbol-file /nix/store/.../vmlinux")

;; 3. Set a hardware breakpoint at start_kernel
(set-hw-breakpoint "start_kernel")

;; 4. Continue and wait for the stop (generous timeout for boot)
(cont-and-wait 120)

;; 5. Inspect
(backtrace)
(list-locals)
```

## Diagnosing silent breakpoint failures

If you accidentally use `(set-temp-breakpoint "start_kernel")` (software)
instead of `(set-hw-breakpoint ...)`, the breakpoint install fails
silently.  `(cont-and-wait)` will time out, and the error message will
include any recent GDB warnings:

```
trigger-and-wait timed out after 30s (target_state=running, ...)
Recent GDB warnings:
  warning: Cannot insert breakpoint 1. Cannot access memory at address 0xffffffff81000000
```

To see warnings proactively, call `(drain-events)` after setting a
breakpoint and before continuing:

```scheme
(set-temp-breakpoint "start_kernel")
(drain-events)  ;; check for "warning:" entries in the log events
```

## See also

- `framewalk://guide/execution-model` — non-stop vs all-stop, `--no-non-stop`
- `framewalk://reference/scheme` — `set-hw-breakpoint`, `drain-events`
- `framewalk://guide/breakpoints` — breakpoint types and options
