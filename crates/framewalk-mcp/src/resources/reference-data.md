# Data tool reference

Tools for evaluating expressions, reading and writing raw memory, and
inspecting registers and disassembly. These operate at the data layer —
no variable-object bookkeeping, no stack walking. For persistent
tracked expressions use `framewalk://reference/variables`.

For concepts read `framewalk://guide/inspection`.

---

## `inspect`

**MI command:** `-data-evaluate-expression`
**Signature:** `inspect(expression: String)`
**Description:** Evaluate an expression in the current frame and return
its value. For one-shot reads; use `watch_create` for expressions you
want to monitor across stops.

```json
{"name": "inspect", "arguments": {"expression": "argv[1]"}}
```

**Related:** `watch_create`, `list_locals`, `framewalk://guide/inspection`

---

## `read_memory`

**MI command:** `-data-read-memory-bytes`
**Signature:** `read_memory(address: String, count: u64, offset: Option<i64>)`
**Description:** Read raw memory bytes from the target. `address` is a
hex literal or expression; `count` is the number of addressable units.
Optional `offset` is relative to `address`. Returns hex-encoded bytes.

```json
{"name": "read_memory", "arguments": {"address": "0x400000", "count": 16}}
```

**Related:** `write_memory`, `disassemble`, `read_memory_deprecated`

---

## `write_memory`

**MI command:** `-data-write-memory-bytes`
**Signature:** `write_memory(address: String, contents: String, count: Option<u64>)`
**Description:** Write hex-encoded bytes to target memory. `contents`
is a hex string. If `count` > the length implied by `contents`, GDB
repeats the pattern to fill `count` bytes.

```json
{"name": "write_memory", "arguments": {"address": "0x400000", "contents": "deadbeef"}}
```

**Related:** `read_memory`

---

## `disassemble`

**MI command:** `-data-disassemble`
**Signature:** `disassemble(start_addr: String, end_addr: String, opcodes: Option<OpcodeMode>, source: bool)`
**Description:** Disassemble a memory range. `start_addr` and `end_addr`
are hex strings or expressions (e.g. `"$pc"`, `"main"`). `opcodes`
controls how raw bytes are shown (`"None"`, `"Bytes"`, `"Display"`).
Set `source: true` to interleave source lines with the disassembly.

```json
{"name": "disassemble", "arguments": {"start_addr": "0x400500", "end_addr": "0x400600", "source": true}}
```

**Related:** `read_memory`, `step_instruction`

---

## `read_registers`

**MI command:** `-data-list-register-values`
**Signature:** `read_registers(format: RegisterFormat, registers: Vec<u32>)`
**Description:** Read register values in the specified format. `format`
is one of `"Hex"`, `"Octal"`, `"Binary"`, `"Decimal"`, `"Raw"`,
`"Natural"`. Pass specific register numbers in `registers`, or an empty
list for all.

```json
{"name": "read_registers", "arguments": {"format": "Hex", "registers": []}}
```

**Related:** `list_register_names`, `list_changed_registers`

---

## `list_register_names`

**MI command:** `-data-list-register-names`
**Signature:** `list_register_names(registers: Vec<u32>)`
**Description:** List register names. Pass specific register numbers,
or an empty list for all. Pair with `read_registers` to map numbers to
names.

```json
{"name": "list_register_names", "arguments": {"registers": []}}
```

**Related:** `read_registers`, `list_changed_registers`

---

## `list_changed_registers`

**MI command:** `-data-list-changed-registers`
**Signature:** `list_changed_registers()` — no arguments
**Description:** List register numbers that changed since the last
stop. Efficient way to highlight state changes after a `step` or `next`.

```json
{"name": "list_changed_registers", "arguments": {}}
```

**Related:** `read_registers`, `list_register_names`

---

## `read_memory_deprecated`

**MI command:** `-data-read-memory`
**Signature:** `read_memory_deprecated(address: String, word_format: MemoryWordFormat, word_size: u32, nr_rows: u32, nr_cols: u32, column_offset: Option<i64>, aschar: Option<String>)`
**Description:** Read target memory in a tabular format (DEPRECATED:
prefer `read_memory` which uses `-data-read-memory-bytes`). Kept for
GDB versions where the newer byte-oriented command is not available.
`word_format` is one of `"Hex"`, `"Decimal"`, `"Octal"`, `"Binary"`,
`"Float"`, `"Character"`, `"String"`, `"Address"`.

```json
{"name": "read_memory_deprecated", "arguments": {"address": "0x400000", "word_format": "Hex", "word_size": 4, "nr_rows": 4, "nr_cols": 4}}
```

**Related:** `read_memory`

---

## See also

- `framewalk://guide/inspection` — reading target state
- `framewalk://guide/no-source` — address-level workflows for stripped binaries
- `framewalk://reference/variables` — persistent watch objects
- `framewalk://reference/stack` — locals and arguments at a frame
- `framewalk://reference/symbols` — looking up addresses and types
- `framewalk://reference/execution` — `step_instruction` for assembly-level work
