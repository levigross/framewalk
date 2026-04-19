# Symbol tool reference

Tools for querying the debug-info symbol table: functions, types,
variables, and Fortran modules. These are read-only queries against
what GDB has loaded via `load_file` / `symbol_file`
(`framewalk://reference/session`) — no target execution is required.

For conceptual guidance on when to grep symbols vs. step through code
read `framewalk://guide/inspection`.

---

## `symbol_info_functions`

**MI command:** `-symbol-info-functions`
**Signature:** `symbol_info_functions(name: Option<String>, type_regexp: Option<String>, include_nondebug: bool, max_results: Option<u32>)`
**Description:** List functions matching optional name/type filters.
`name` and `type_regexp` are regular expressions. `include_nondebug`
adds symbols without debug info (e.g. libc exports).

```json
{"name": "symbol_info_functions", "arguments": {"name": "^parse_", "include_nondebug": false}}
```

**Related:** `symbol_info_variables`, `symbol_info_types`

---

## `symbol_info_types`

**MI command:** `-symbol-info-types`
**Signature:** `symbol_info_types(name: Option<String>, type_regexp: Option<String>, include_nondebug: bool, max_results: Option<u32>)`
**Description:** List types matching optional name filter. Only the
`name` field is used; `type_regexp` and `include_nondebug` are
accepted but unused for types.

```json
{"name": "symbol_info_types", "arguments": {"name": "Config", "include_nondebug": false}}
```

**Related:** `symbol_info_functions`, `symbol_info_variables`

---

## `symbol_info_variables`

**MI command:** `-symbol-info-variables`
**Signature:** `symbol_info_variables(name: Option<String>, type_regexp: Option<String>, include_nondebug: bool, max_results: Option<u32>)`
**Description:** List variables matching optional name/type filters.

```json
{"name": "symbol_info_variables", "arguments": {"name": "^g_", "type_regexp": "int", "include_nondebug": false}}
```

**Related:** `symbol_info_functions`, `list_variables`

---

## `symbol_info_modules`

**MI command:** `-symbol-info-modules`
**Signature:** `symbol_info_modules(name: Option<String>, type_regexp: Option<String>, include_nondebug: bool, max_results: Option<u32>)`
**Description:** List Fortran modules matching optional name filter.
Only `name` is meaningful for modules.

```json
{"name": "symbol_info_modules", "arguments": {"name": "physics", "include_nondebug": false}}
```

**Related:** `symbol_info_module_functions`, `symbol_info_module_variables`

---

## `symbol_info_module_functions`

**MI command:** `-symbol-info-module-functions`
**Signature:** `symbol_info_module_functions(module: Option<String>, name: Option<String>, type_regexp: Option<String>)`
**Description:** List functions defined in Fortran modules. `module`
filters by module name regexp; `name` filters by function name.

```json
{"name": "symbol_info_module_functions", "arguments": {"module": "physics", "name": "compute_"}}
```

**Related:** `symbol_info_modules`, `symbol_info_module_variables`

---

## `symbol_info_module_variables`

**MI command:** `-symbol-info-module-variables`
**Signature:** `symbol_info_module_variables(module: Option<String>, name: Option<String>, type_regexp: Option<String>)`
**Description:** List variables defined in Fortran modules.

```json
{"name": "symbol_info_module_variables", "arguments": {"module": "physics"}}
```

**Related:** `symbol_info_modules`, `symbol_info_module_functions`

---

## `symbol_list_lines`

**MI command:** `-symbol-list-lines`
**Signature:** `symbol_list_lines(filename: String)`
**Description:** List line number entries for a source file. Returns
each source line that has a corresponding code address — useful for
setting breakpoints only on real statements.

```json
{"name": "symbol_list_lines", "arguments": {"filename": "main.c"}}
```

**Related:** `list_source_files`, `set_breakpoint`

---

## See also

- `framewalk://guide/inspection` — when to query symbols
- `framewalk://guide/no-source` — using `include_nondebug` and address workflows on stripped binaries
- `framewalk://reference/session` — loading symbol files
- `framewalk://reference/stack` — runtime variable listings
- `framewalk://reference/breakpoints` — setting breakpoints by name
