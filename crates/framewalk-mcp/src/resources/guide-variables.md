# Variable objects: persistent watches across stops

## When to use this guide

Read this when you need to track one or more expressions across many stops
without re-typing them and without re-evaluating the full expression each
time. Variable objects (GDB's "varobjs") are the mechanism: you create a
handle once with `watch_create`, then cheaply ask for its current value,
expand its children, check whether it has changed, or poll whether it is
still in scope. They replace repeated `inspect` calls for the values you
care about on every stop. For the full tool catalog see
`framewalk://reference/variables`; for one-shot expression evaluation at a
single stop, use `inspect` (see `framewalk://guide/inspection`).

**Two polling tools, two jobs.** `watch_list` asks GDB "which of my varobjs
changed since the last poll?" — it advances the changed-watermark across
every live varobj and returns the subset that moved. `var_evaluate_expression`
reads one specific varobj's current value without affecting the watermark.
Use `watch_list` at the top of each stop to see what moved; use
`var_evaluate_expression` to re-read a single value you already know you
care about.

## Lifetime and scope model

A variable object is created in the context of the current frame at a
stop. GDB remembers the expression, the frame, and a type. From that point
on:

- `watch_list` returns the subset of varobjs whose displayed value has
  changed since the last call — the cheap "what moved" query. It is the
  core of any multi-step watching workflow. Takes no arguments; always
  polls every live varobj.
- `var_evaluate_expression` returns the current value string of one varobj
  by name, without affecting the changed-watermark.
- `var_list_children` returns child varobjs for aggregates (struct fields,
  array elements, pointer targets).
- A varobj is "in scope" when the frame it was created in is still on the
  stack. If you step out of that frame the varobj stays allocated but its
  `in_scope` field becomes `"false"` and its value is undefined. If you
  re-enter a frame at the same function, GDB tries to rebind it.
- The handle lives until you call `watch_delete` on it, or until
  `enable_pretty_printing` / certain session-level operations invalidate it.

```json
{"name": "watch_create", "arguments": {"expression": "req->body_len"}}
{"name": "watch_create", "arguments": {"expression": "*this"}}
```

GDB assigns an auto name like `var1`, `var2`, … Record the returned name
from the `^done` reply; subsequent calls (`var_evaluate_expression`,
`var_list_children`, `watch_delete`) take it as their `name` argument.

## Polling for changes

```json
{"name": "watch_list", "arguments": {}}
```

`watch_list` checks every live varobj and returns the ones whose displayed
value changed since the previous call. The response includes the new value
and the `in_scope` state. A typical loop:

```json
{"name": "cont", "arguments": {}}
{"name": "watch_list", "arguments": {}}
{"name": "cont", "arguments": {}}
{"name": "watch_list", "arguments": {}}
```

To re-read a specific varobj's value without advancing the watermark
(e.g. to sanity-check a value you already know is important), use
`var_evaluate_expression` with its name.

## Expanding aggregates

For structs, arrays, and pointer-to-struct varobjs, the children are not
created eagerly — you request them.

```json
{"name": "var_list_children", "arguments": {"name": "var1"}}
```

The returned children have auto-generated names like `var1.field1` or
`var1.*` for a pointer dereference. Each child is itself a full varobj —
you can call `var_evaluate_expression`, `var_list_children`, `var_assign`
on it, and `watch_list` will see it next time a child's value changes.

`var_info_num_children` returns the count without materializing them.
`var_set_update_range` restricts updates to a subslice of an array varobj's
children — essential for huge arrays.

## Reading metadata

```json
{"name": "var_info_type", "arguments": {"name": "var1"}}
{"name": "var_info_expression", "arguments": {"name": "var1.field1"}}
{"name": "var_info_path_expression", "arguments": {"name": "var1.field1"}}
{"name": "var_show_attributes", "arguments": {"name": "var1"}}
```

`var_info_expression` returns the short expression relative to the parent.
`var_info_path_expression` returns a fully-qualified C expression you can
pass back to `inspect` — important when you want to capture a child as a
standalone probe.

## Display formatting

```json
{"name": "var_set_format", "arguments": {"name": "var1.field1", "format": "Hexadecimal"}}
{"name": "var_show_format", "arguments": {"name": "var1.field1"}}
```

Formats are `"Natural"` (GDB's default), `"Binary"`, `"Decimal"`,
`"Hexadecimal"`, `"Octal"`, `"ZeroHexadecimal"`. The setting is
per-varobj and persists until changed.

## Writing values

```json
{"name": "var_assign", "arguments": {"name": "self.field1", "expression": "42"}}
```

Assignment writes into the target's memory. Subject to the same caveats as
`write_memory` — you can corrupt target state.

## Freezing and unfreezing

```json
{"name": "var_set_frozen", "arguments": {"name": "var1", "frozen": true}}
```

A frozen varobj is skipped by `watch_list` — its displayed value does not
refresh automatically. Use it to pin a snapshot you want to compare against
later, or to pause a very expensive varobj (deeply-nested struct with a
custom pretty-printer) during stepping.

## Pretty-printing

```json
{"name": "enable_pretty_printing", "arguments": {}}
```

Enables GDB's Python pretty-printer system for varobj display. After this
call, a `std::vector<int>` varobj's children are its elements rather than
its internal `_M_start` / `_M_finish` fields. Call this early — existing
varobjs may need to be recreated to pick up the new format.

`var_set_visualizer` overrides the pretty-printer for a specific varobj.

## Common pitfalls

- **Frame-bound lifetime.** A varobj created at frame 0 inside `parse_body`
  becomes out-of-scope the moment you `finish` out of `parse_body`. It is
  still allocated and `watch_list` will report it — with `in_scope:
  "false"` — but its value is meaningless. Delete it, or recreate it on
  re-entry.

- **Auto-allocated child names are opaque.** Names like `var1.0.1.children`
  are generated by GDB's internal numbering and depend on the order you
  called `var_list_children`. Do not parse them. Store the names GDB gives
  you in the `watch_list` / `var_list_children` results and use them
  verbatim.

- **`in_scope` flips between polls.** On a recursive function, the same
  varobj may be in scope, out of scope, and in scope again across three
  consecutive stops as the recursive call winds and unwinds. Code against
  `in_scope` on every `watch_list`, do not assume stability.

- **Pretty-printing changes child layout mid-session.** Toggling
  `enable_pretty_printing` after creating varobjs can leave old varobjs in
  an inconsistent state. Enable it immediately after `load_file`, before
  creating any watches.

- **`watch_list` is O(live varobjs).** On a session with hundreds of
  varobjs, the global poll gets expensive. Freeze or delete the ones you
  are not actively watching, or skip `watch_list` entirely and re-read a
  specific varobj via `var_evaluate_expression`.

- **Expression evaluation side effects.** `watch_create "obj.size()"` calls
  `size()` *every* `watch_list`. If `size()` takes a lock or is slow,
  every stop incurs that cost. Prefer field expressions over function
  calls.

- **`var_assign` to a const.** Assigning to a `const`-qualified varobj
  silently fails on some GDB builds and returns an error on others. Cast
  in the expression first: create the watch as
  `"*((int*)&const_value)"`.

## Example session

```json
{"name": "enable_pretty_printing", "arguments": {}}
{"name": "set_breakpoint", "arguments": {"location": "parse_request"}}
{"name": "run", "arguments": {}}
{"name": "watch_create", "arguments": {"expression": "*req"}}
{"name": "var_list_children", "arguments": {"name": "var1"}}
{"name": "var_set_format", "arguments": {"name": "var1.body_len", "format": "Hexadecimal"}}
{"name": "cont", "arguments": {}}
{"name": "watch_list", "arguments": {}}
{"name": "var_evaluate_expression", "arguments": {"name": "var1.body_len"}}
{"name": "var_assign", "arguments": {"name": "var1.body_len", "expression": "0"}}
{"name": "cont", "arguments": {}}
{"name": "watch_delete", "arguments": {"name": "var1"}}
```

The session enables pretty-printing, stops at the parser, creates a
structured watch on the request, lists its fields, formats one as hex,
continues to the next stop, polls for changes, overwrites the body length
to force an empty-body code path, continues, and finally cleans up.

## See also

- `framewalk://reference/variables` — full tool catalog and schemas
- `framewalk://guide/inspection` — one-shot evaluation via `inspect`
- `framewalk://guide/execution` — stepping patterns that pair with watches
- `framewalk://guide/execution-model` — when varobj state is observable
- `framewalk://recipe/conditional-breakpoint` — watches + conditions
