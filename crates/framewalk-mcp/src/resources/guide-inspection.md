# Inspection: reading program state at a stop

## When to use this guide

Read this once the target is stopped — at a breakpoint, after a step, on a
signal — and you need to understand what the program was doing. This guide
is structured as "what do you want to know? here is the tool". It covers
stack walking, frame navigation, locals and arguments, arbitrary expression
evaluation, raw memory, and CPU registers. For execution control see
`framewalk://guide/execution`; for the full data-inspection tool catalog see
`framewalk://reference/data`, `framewalk://reference/stack`, and
`framewalk://reference/threads`.

## What function am I in? What called it?

`backtrace` returns the full call stack of the current thread.

```json
{"name": "backtrace", "arguments": {}}
{"name": "backtrace", "arguments": {"limit": 10}}
```

Each frame carries its function name, source location, and level. Pass
`limit` to cap the walk at the N innermost frames on deep stacks — useful
when the full backtrace is large and you only need the recent frames. To
see locals for a specific frame, call `list_locals` after `select_frame`
(or `list_variables` for locals + arguments in one call).

`stack_depth` returns the number of frames if you only need the count.

`frame_info` returns metadata for the currently-selected frame. There are
no arguments — switch frames with `select_frame` first, then call
`frame_info` to confirm where you are.

```json
{"name": "frame_info", "arguments": {}}
```

## Walking up the stack

`select_frame` switches the "current" frame for subsequent commands. Frame 0
is the innermost (where execution stopped); higher numbers walk toward
`main`.

```json
{"name": "select_frame", "arguments": {"level": 2}}
```

After selecting, `list_locals`, `list_arguments`, and `inspect` all operate
in that frame's context. This is the standard pattern for crash
investigation — start at frame 0, look for the cause, walk up if the cause
is in a caller.

## What are the locals here?

```json
{"name": "list_locals", "arguments": {"print_values": "AllValues"}}
{"name": "list_arguments", "arguments": {"print_values": "AllValues"}}
{"name": "list_variables", "arguments": {"print_values": "AllValues"}}
```

`list_locals` returns the function's local variables. `list_arguments`
returns its parameters. `list_variables` returns both in one call. Each
requires a `print_values` detail level — `"AllValues"` returns current
values, `"SimpleValues"` returns values only for scalar types (faster on
deep stacks), `"NoValues"` returns names and types only.

## What is the value of this expression?

`inspect` evaluates a C/C++/Rust expression in the current frame.

```json
{"name": "inspect", "arguments": {"expression": "argc"}}
{"name": "inspect", "arguments": {"expression": "ptr->next->value"}}
{"name": "inspect", "arguments": {"expression": "sizeof(*ptr)"}}
{"name": "inspect", "arguments": {"expression": "(int[4])*arr"}}
```

The expression language is GDB's — most of C plus casts, dereference,
subscripting, address-of, arithmetic. You can call functions (`strlen(s)`),
cast (`(char*)p`), and read globals by name. The result is formatted as a
string; for a live handle that updates across stops use the variable-object
family (`framewalk://guide/variables`).

## What is at this address?

`read_memory` dumps raw bytes. Useful for inspecting buffers, checking
guard bytes, or dumping structures GDB cannot print.

```json
{"name": "read_memory", "arguments": {"address": "0x7fffffffd000", "count": 64}}
{"name": "read_memory", "arguments": {"address": "&g_state", "count": 128, "offset": 16}}
```

`count` is the number of addressable memory units (bytes on common
architectures); `offset` is applied relative to `address` before the read
starts.

`write_memory` is the inverse — patch bytes at an address. Use for
error-injection experiments; see the warnings in
`framewalk://reference/data`.

## What instructions are running here?

`disassemble` returns instructions in an address range.

```json
{"name": "disassemble", "arguments": {"start_addr": "$pc", "end_addr": "$pc + 64"}}
{"name": "disassemble", "arguments": {"start_addr": "parse_header", "end_addr": "parse_header + 128", "source": true}}
```

Pass `source: true` to interleave source lines with instructions — the
best view for debugging optimizer behavior. `opcodes` (`"None"`,
`"Bytes"`, or `"Display"`) controls how raw instruction bytes are shown
alongside the mnemonics.

## What are the registers?

```json
{"name": "list_register_names", "arguments": {}}
{"name": "read_registers", "arguments": {"format": "Hex"}}
{"name": "read_registers", "arguments": {"format": "Natural", "registers": [0, 4, 5]}}
{"name": "list_changed_registers", "arguments": {}}
```

`list_register_names` returns every register the current architecture
exposes in slot order — this is the authoritative list, and it differs
per CPU (x86_64 has ~80, ARM64 is different, RISC-V different again).
Use it once to map names to the numeric slots you will pass to
`read_registers`. `read_registers` requires a `format`
(`"Hex"`, `"Octal"`, `"Binary"`, `"Decimal"`, `"Raw"`, or `"Natural"`)
and takes register *numbers*, not names — omit `registers` to read all.

`list_changed_registers` returns only those that changed since the last
stop — invaluable when single-stepping to see the effect of each
instruction.

## Which thread? What other threads are there?

```json
{"name": "list_threads", "arguments": {}}
{"name": "thread_list_ids", "arguments": {}}
{"name": "select_thread", "arguments": {"thread_id": "3"}}
```

`list_threads` returns full info for each thread (state, current frame,
target ID). `thread_list_ids` returns just the IDs — faster on processes
with hundreds of threads. After `select_thread`, all inspection tools
operate on that thread's stack.

## Common pitfalls

- **`<optimized out>`.** Optimized builds drop locals whose values are no
  longer needed into registers that are then reused. GDB prints
  `<optimized out>` for these. There is no workaround other than rebuilding
  with lower optimization, or inferring the value from a register dump at
  the time of interest. Check `read_registers` and the preceding
  instructions with `disassemble` — you can often recover the value
  manually.

- **Expression side effects.** `inspect "obj.size()"` calls `size()` in the
  target, which may take a lock, allocate, or crash if the object is
  half-constructed. At a crash stop, use `var_info_type` (see
  `framewalk://guide/variables`) or cast to a plain struct and read fields
  directly: `inspect "*(struct vec*)&obj"`.

- **Disassembling unaligned addresses.** `disassemble` starting
  mid-instruction yields garbage. Use a symbol expression
  (`start_addr: "parse_header"`) or `$pc` at a stop — both are
  guaranteed-aligned.

- **Register numbering is architecture-specific.** The slot index for `rax`
  on x86_64 is not the slot index for any register on ARM. Always call
  `list_register_names` first if you are writing a cross-architecture
  helper, then look up the index of the register you want before calling
  `read_registers`.

- **Frame selection is sticky.** After `select_frame 3`, the next `cont`
  still resumes the thread from frame 0 — selection only affects *query*
  tools. But if you forget you selected, `list_locals` keeps returning the
  caller's locals long after you expected to be back at the innermost
  frame. Call `select_frame 0` to reset, or issue any stepping command
  (which auto-selects frame 0).

- **`list_locals` in optimized code returns fewer entries than the source
  suggests.** Variables may not exist as distinct entities. Cross-check
  against the disassembly.

- **Reading memory across a page boundary.** If the target unmapped the
  second page, `read_memory` fails for the whole range. Read smaller
  chunks.

## Example session

```json
{"name": "backtrace", "arguments": {}}
{"name": "frame_info", "arguments": {}}
{"name": "list_locals", "arguments": {"print_values": "SimpleValues"}}
{"name": "inspect", "arguments": {"expression": "req->body_len"}}
{"name": "select_frame", "arguments": {"level": 2}}
{"name": "list_arguments", "arguments": {"print_values": "AllValues"}}
{"name": "inspect", "arguments": {"expression": "path"}}
{"name": "read_memory", "arguments": {"address": "path", "count": 64}}
{"name": "disassemble", "arguments": {"start_addr": "$pc - 32", "end_addr": "$pc + 32"}}
{"name": "read_registers", "arguments": {"format": "Hex"}}
```

The backtrace orients you. Frame 0 locals and a specific field expose the
immediate state. Walking up two frames reveals which caller supplied the
bad input. A raw memory dump of that input confirms the content. The
disassembly and register snapshot capture exactly what the CPU was about to
do.

## See also

- `framewalk://guide/no-source` — working from addresses, memory, and registers when symbols are missing
- `framewalk://reference/stack` — `backtrace`, `frame_info`, `select_frame`, locals
- `framewalk://reference/data` — `inspect`, memory, disassembly, registers
- `framewalk://reference/threads` — `list_threads`, `select_thread`
- `framewalk://guide/variables` — variable objects for persistent watches
- `framewalk://guide/execution` — advancing after inspection
