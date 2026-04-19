# Debugging without source or with a stripped binary

This guide covers two related but different situations:

- you do not have the source tree for the build you are debugging
- the binary is stripped, so debug symbols, line tables, or local-variable
  locations are missing

Both cases are still debuggable with framewalk, but the workflow shifts
from source lines and locals toward addresses, exported symbols,
disassembly, registers, and raw memory.

This page is also exposed to MCP clients as `framewalk://guide/no-source`.

## First principles

There are three independent assets in a source-level debug session:

1. The executable code that actually runs.
2. Symbolic metadata such as function names, file names, line tables,
   variable locations, and type info.
3. The source files on disk.

`load_file` gives GDB the executable. If debug metadata is present, GDB can
still answer many source-level questions even when the source files are not
on disk. If the binary is stripped, execution control still works, but
anything that depends on debug metadata becomes weak or unavailable.

That gives you three practical cases.

## Case 1: debug info exists, but the source tree is unavailable

This is the least painful case. GDB still knows about functions, line
numbers, frames, and often locals, even if you cannot open the source
paths it references.

Usually still useful:

- `backtrace`
- `frame_info`
- `list_locals`, `list_arguments`, `list_variables`
- `inspect`
- `symbol_info_functions`, `symbol_info_types`, `symbol_info_variables`
- `list_source_files`

Typical workflow:

```json
{"name": "load_file", "arguments": {"path": "/opt/bin/service"}}
{"name": "set_breakpoint", "arguments": {"location": "main"}}
{"name": "run", "arguments": {}}
{"name": "backtrace", "arguments": {}}
{"name": "list_locals", "arguments": {"print_values": "SimpleValues"}}
```

If you want to see what source paths were baked into the build, use:

```json
{"name": "list_source_files", "arguments": {}}
```

The source files might not exist locally, but the recorded paths still help
you reason about the build and map it back to a repository or release.

## Case 2: the executable is stripped, but you have a separate debug-symbol file

This is common for distro packages and production deployments. The shipped
binary is stripped, but the matching `.debug` file or unstripped build
exists elsewhere.

Load both:

```json
{"name": "load_file", "arguments": {"path": "/opt/bin/service"}}
{"name": "symbol_file", "arguments": {"path": "/srv/debug/service.debug"}}
```

Once the matching symbol file is loaded, the session behaves much more like
a normal source-level debug session.

Hard constraint: the symbol file has to match the exact build. A “close
enough” symbol file is worse than none because it can produce plausible but
wrong answers for frames, line mappings, and variable locations.

## Case 3: the executable is stripped and you do not have separate symbols

This is the hardest case. Assume you are doing address-level and
instruction-level debugging with whatever symbol crumbs remain in the
dynamic symbol table.

Your primary tools become:

- `backtrace`
- `frame_info`
- `set_breakpoint` with raw addresses
- `disassemble`
- `list_register_names`
- `read_registers`
- `list_changed_registers`
- `read_memory`
- `symbol_info_functions(..., include_nondebug: true)`

Weak or unavailable tools in this scenario:

- `list_locals`
- `list_arguments`
- `list_variables`
- `symbol_info_types`
- `symbol_info_variables`
- `symbol_list_lines`

If the answer needs debug metadata, a fully stripped binary cannot invent
it.

## Recover whatever symbol names still exist

Stripped binaries often still expose exported symbols from the dynamic
symbol table. Those are not full debug symbols, but they can still give you
anchors such as `main`, `init`, or library entry points.

Try:

```json
{"name": "symbol_info_functions", "arguments": {"include_nondebug": true, "max_results": 50}}
{"name": "symbol_info_functions", "arguments": {"name": "main|parse|init", "include_nondebug": true}}
```

If the result set is thin, move fully to address-based debugging.

## Set breakpoints by address

`set_breakpoint` accepts raw addresses by prefixing them with `*`.

```json
{"name": "set_breakpoint", "arguments": {"location": "*0x4012f0"}}
```

Use this when:

- a crash log gives you an instruction pointer
- a frame shows only an address
- disassembly reveals a branch target or call site you want to trap

Without source or full symbols, address breakpoints are the normal path.

## Disassemble around the current PC

When line tables are missing, the instruction pointer is the most reliable
anchor. After a stop:

```json
{"name": "frame_info", "arguments": {}}
{"name": "disassemble", "arguments": {"start_addr": "$pc-32", "end_addr": "$pc+64", "source": false}}
```

This answers the questions that matter most in stripped-binary triage:

- what instruction faulted?
- was it a load, store, branch, or call?
- which register should have held the pointer?
- where does execution go next?

## Read registers the right way

`read_registers` takes register **numbers**, not names. The correct flow is:

1. Call `list_register_names`.
2. Map the architecture’s register numbers to the names you care about.
3. Call `read_registers` with those numbers.

Example:

```json
{"name": "list_register_names", "arguments": {}}
{"name": "read_registers", "arguments": {"format": "Hex", "registers": [0, 1, 2, 7]}}
```

`list_changed_registers` is useful after stepping one instruction to see
what actually changed:

```json
{"name": "step_instruction", "arguments": {}}
{"name": "list_changed_registers", "arguments": {}}
```

This matters because stripped-binary work is often architecture-sensitive,
and register names vary across x86_64, AArch64, ARM, and RISC-V.

## Read raw memory instead of assuming type information

Without debug types, raw memory becomes more valuable than local-variable
queries.

```json
{"name": "read_memory", "arguments": {"address": "$sp", "count": 128}}
{"name": "read_memory", "arguments": {"address": "0x7fffffffdc80", "count": 64}}
```

This helps you:

- inspect stack contents near the crash
- validate whether a pointer target looks plausible
- recover strings, packet headers, or small structs
- cross-check what the disassembly says the code is reading or writing

If you know the ABI or structure layout from external context, raw memory
plus disassembly is often enough to reconstruct the failure.

## Minimal stripped-binary crash workflow

1. `load_file`
2. run or attach
3. `backtrace`
4. `frame_info`
5. `disassemble` around `$pc`
6. `list_register_names`
7. `read_registers`
8. `read_memory` at the suspicious address or near `$sp`
9. if you find a stable address of interest, set a breakpoint there and rerun

Example:

```json
{"name": "load_file", "arguments": {"path": "/opt/bin/service"}}
{"name": "run", "arguments": {}}
{"name": "backtrace", "arguments": {}}
{"name": "frame_info", "arguments": {}}
{"name": "disassemble", "arguments": {"start_addr": "$pc-32", "end_addr": "$pc+64"}}
{"name": "list_register_names", "arguments": {}}
{"name": "read_registers", "arguments": {"format": "Hex", "registers": [0, 1, 2, 7]}}
{"name": "read_memory", "arguments": {"address": "$sp", "count": 96}}
```

## External addresses are enough

If you have addresses from logs, telemetry, `objdump`, `nm`, a map file,
or a reverse-engineering notebook, you can still drive framewalk directly:

- set breakpoints at `*0x...`
- disassemble around `0x...`
- read memory near `0x...`

Framewalk does not require source files to be useful. It requires a stable
way to address program state, and addresses are enough.

## Scheme mode is especially useful here

Stripped-binary debugging often turns into repetitive low-level sampling:

- step N instructions
- capture changed registers each time
- dump memory windows on each stop
- stop when a register reaches a value or an address range

That is exactly the kind of loop `scheme_eval` is good at. If the session
becomes repetitive and assembly-heavy, keep the loop inside one Scheme call
instead of round-tripping tool-by-tool.

## See also

- [Getting Started](getting-started.md)
- [Modes](modes.md)
- `framewalk://guide/inspection`
- `framewalk://reference/data`
- `framewalk://reference/symbols`
- `framewalk://reference/session`
- `framewalk://guide/attach`
