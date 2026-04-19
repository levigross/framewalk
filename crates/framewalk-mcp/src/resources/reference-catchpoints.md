# Catchpoint tool reference

Catchpoints stop the target on events other than reaching a code
location: shared-library loads/unloads, C++ exception throws, and Ada
exceptions and assertions. They appear alongside regular breakpoints in
`list_breakpoints` and are managed by the same enable/disable/delete
tools in `framewalk://reference/breakpoints`.

For the conceptual difference between breakpoints and catchpoints read
`framewalk://guide/breakpoints`.

---

## `catch_load`

**MI command:** `-catch-load`
**Signature:** `catch_load(regexp: String, temporary: bool, disabled: bool)`
**Description:** Catch shared library loads matching a regexp. Stops
the target whenever the dynamic linker maps a matching library. Useful
for breaking into a plugin the moment it loads.

```json
{"name": "catch_load", "arguments": {"regexp": "libssl", "temporary": false, "disabled": false}}
```

**Related:** `catch_unload`, `list_shared_libraries`

---

## `catch_unload`

**MI command:** `-catch-unload`
**Signature:** `catch_unload(regexp: String, temporary: bool, disabled: bool)`
**Description:** Catch shared library unloads matching a regexp.

```json
{"name": "catch_unload", "arguments": {"regexp": "libplugin", "temporary": false, "disabled": false}}
```

**Related:** `catch_load`, `list_shared_libraries`

---

## `catch_assert`

**MI command:** `-catch-assert`
**Signature:** `catch_assert(condition: Option<String>, disabled: bool, temporary: bool, exception_name: Option<String>, unhandled: bool)`
**Description:** Catch failed Ada assertions. `exception_name` and
`unhandled` are unused for assertions but share the underlying struct
with `catch_exception` / `catch_handlers`.

```json
{"name": "catch_assert", "arguments": {"temporary": false, "disabled": false, "unhandled": false}}
```

**Related:** `catch_exception`, `catch_handlers`

---

## `catch_exception`

**MI command:** `-catch-exception`
**Signature:** `catch_exception(condition: Option<String>, disabled: bool, temporary: bool, exception_name: Option<String>, unhandled: bool)`
**Description:** Catch Ada exceptions (optionally filtering by name or
unhandled). Pass `exception_name` to stop only for a specific exception,
or `unhandled: true` to stop only when no handler exists.

```json
{"name": "catch_exception", "arguments": {"exception_name": "Constraint_Error", "temporary": false, "disabled": false, "unhandled": false}}
```

**Related:** `catch_assert`, `catch_handlers`, `info_ada_exceptions`

---

## `catch_handlers`

**MI command:** `-catch-handlers`
**Signature:** `catch_handlers(condition: Option<String>, disabled: bool, temporary: bool, exception_name: Option<String>, unhandled: bool)`
**Description:** Catch Ada exception handlers. Stops when a matching
handler begins executing.

```json
{"name": "catch_handlers", "arguments": {"exception_name": "Constraint_Error", "temporary": false, "disabled": false, "unhandled": false}}
```

**Related:** `catch_exception`, `catch_assert`

---

## `catch_throw`

**MI command:** `-catch-throw`
**Signature:** `catch_throw(temporary: bool, regexp: Option<String>)`
**Description:** Catch C++ exception throws. Optional `regexp` filters
by exception type name.

```json
{"name": "catch_throw", "arguments": {"temporary": false, "regexp": "std::runtime_error"}}
```

**Related:** `catch_rethrow`, `catch_catch`

---

## `catch_rethrow`

**MI command:** `-catch-rethrow`
**Signature:** `catch_rethrow(temporary: bool, regexp: Option<String>)`
**Description:** Catch C++ exception rethrows. Optional `regexp`
filters by exception type name.

```json
{"name": "catch_rethrow", "arguments": {"temporary": false}}
```

**Related:** `catch_throw`, `catch_catch`

---

## `catch_catch`

**MI command:** `-catch-catch`
**Signature:** `catch_catch(temporary: bool, regexp: Option<String>)`
**Description:** Catch C++ exception catches — stops when a matching
`catch` clause begins executing. Optional `regexp` filters by exception
type name.

```json
{"name": "catch_catch", "arguments": {"temporary": false, "regexp": "std::.*"}}
```

**Related:** `catch_throw`, `catch_rethrow`

---

## See also

- `framewalk://guide/breakpoints` — breakpoints and catchpoints
- `framewalk://reference/breakpoints` — managing catchpoints (enable, delete, list)
- `framewalk://reference/execution` — continuing after a catchpoint fires
- `framewalk://reference/stack` — inspecting state at a caught event
