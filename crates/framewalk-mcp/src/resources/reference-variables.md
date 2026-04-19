# Variable object tool reference

GDB variable objects ("varobjs") are handles that track an expression
across stops — GDB re-evaluates them automatically, so you can detect
changes without re-submitting the expression every time. Use varobjs
for anything you want to *watch*; use `inspect`
(`framewalk://reference/data`) for one-shot reads.

For the full model — how names are allocated, how the tree of children
works, what `print_values` means — read `framewalk://guide/variables`.

---

## `watch_create`

**MI command:** `-var-create`
**Signature:** `watch_create(expression: String)`
**Description:** Create a GDB variable object that watches the given
expression. The varobj is anchored at the current frame; GDB
auto-allocates a name (e.g. `var1`, `var2`), which the `^done` reply
includes — record it and pass it to subsequent
`var_evaluate_expression`, `var_list_children`, and `watch_delete`
calls.

```json
{"name": "watch_create", "arguments": {"expression": "my_struct->count"}}
```

**Related:** `watch_list`, `watch_delete`, `var_evaluate_expression`

---

## `watch_list`

**MI command:** `-var-update`
**Signature:** `watch_list()` — no arguments
**Description:** List currently-active GDB variable objects and their
changed state. Uses `-var-update *` under the hood to refresh all
varobjs.

```json
{"name": "watch_list", "arguments": {}}
```

**Related:** `watch_create`, `var_evaluate_expression`

---

## `watch_delete`

**MI command:** `-var-delete`
**Signature:** `watch_delete(name: String)`
**Description:** Delete a variable object by name. The name is the
auto-allocated string GDB returned from `watch_create`.

```json
{"name": "watch_delete", "arguments": {"name": "var1"}}
```

**Related:** `watch_create`, `watch_list`

---

## `var_list_children`

**MI command:** `-var-list-children`
**Signature:** `var_list_children(name: String, print_values: Option<PrintValues>, from: Option<u32>, to: Option<u32>)`
**Description:** List children of a variable object (for expanding
aggregates/arrays). Pass `from` and `to` together to request a slice
of a large array. `print_values` controls whether child values are
inlined.

```json
{"name": "var_list_children", "arguments": {"name": "var1", "print_values": "AllValues"}}
```

**Related:** `var_info_num_children`, `var_set_update_range`

---

## `var_evaluate_expression`

**MI command:** `-var-evaluate-expression`
**Signature:** `var_evaluate_expression(name: String)`
**Description:** Evaluate a variable object and return its current
value. Cheaper than `watch_list` when you already know which varobj
you care about.

```json
{"name": "var_evaluate_expression", "arguments": {"name": "var1"}}
```

**Related:** `watch_list`, `var_info_expression`

---

## `var_assign`

**MI command:** `-var-assign`
**Signature:** `var_assign(name: String, expression: String)`
**Description:** Assign a new value to a variable object. The varobj
must be marked editable by GDB (see `var_show_attributes`).

```json
{"name": "var_assign", "arguments": {"name": "var1", "expression": "42"}}
```

**Related:** `var_show_attributes`, `var_evaluate_expression`

---

## `var_set_format`

**MI command:** `-var-set-format`
**Signature:** `var_set_format(name: String, format: VarFormat)`
**Description:** Set the display format of a variable object. `format`
is `"Binary"`, `"Decimal"`, `"Hexadecimal"`, `"Octal"`, `"Natural"`,
or `"ZeroHexadecimal"`.

```json
{"name": "var_set_format", "arguments": {"name": "var1", "format": "Hexadecimal"}}
```

**Related:** `var_show_format`, `var_evaluate_expression`

---

## `var_show_format`

**MI command:** `-var-show-format`
**Signature:** `var_show_format(name: String)`
**Description:** Show the current display format of a variable object.

```json
{"name": "var_show_format", "arguments": {"name": "var1"}}
```

**Related:** `var_set_format`

---

## `var_info_num_children`

**MI command:** `-var-info-num-children`
**Signature:** `var_info_num_children(name: String)`
**Description:** Return the number of children of a variable object.
Useful to decide whether to call `var_list_children` and how many
slices to request.

```json
{"name": "var_info_num_children", "arguments": {"name": "var1"}}
```

**Related:** `var_list_children`

---

## `var_info_type`

**MI command:** `-var-info-type`
**Signature:** `var_info_type(name: String)`
**Description:** Return the type of a variable object as a string.

```json
{"name": "var_info_type", "arguments": {"name": "var1"}}
```

**Related:** `var_info_expression`, `symbol_info_types`

---

## `var_info_expression`

**MI command:** `-var-info-expression`
**Signature:** `var_info_expression(name: String)`
**Description:** Return the expression that a variable object
represents.

```json
{"name": "var_info_expression", "arguments": {"name": "var1"}}
```

**Related:** `var_info_path_expression`

---

## `var_info_path_expression`

**MI command:** `-var-info-path-expression`
**Signature:** `var_info_path_expression(name: String)`
**Description:** Return the full path expression for a variable object
(for use in other GDB commands). Useful when you want to pass a child
of a varobj to `inspect` or `set_watchpoint`.

```json
{"name": "var_info_path_expression", "arguments": {"name": "var1.child"}}
```

**Related:** `var_info_expression`, `inspect`

---

## `var_show_attributes`

**MI command:** `-var-show-attributes`
**Signature:** `var_show_attributes(name: String)`
**Description:** Show whether a variable object is editable. Checks
the `editable` attribute, which gates `var_assign`.

```json
{"name": "var_show_attributes", "arguments": {"name": "var1"}}
```

**Related:** `var_assign`

---

## `var_set_frozen`

**MI command:** `-var-set-frozen`
**Signature:** `var_set_frozen(name: String, frozen: bool)`
**Description:** Freeze or thaw a variable object (frozen objects skip
updates). Frozen varobjs retain their last value until thawed.

```json
{"name": "var_set_frozen", "arguments": {"name": "var1", "frozen": true}}
```

**Related:** `watch_list`

---

## `var_set_update_range`

**MI command:** `-var-set-update-range`
**Signature:** `var_set_update_range(name: String, from: u32, to: u32)`
**Description:** Set the child range that `-var-update` refreshes.
Efficient for huge arrays where you only care about a small window.

```json
{"name": "var_set_update_range", "arguments": {"name": "var1", "from": 0, "to": 100}}
```

**Related:** `var_list_children`, `watch_list`

---

## `var_set_visualizer`

**MI command:** `-var-set-visualizer`
**Signature:** `var_set_visualizer(name: String, visualizer: String)`
**Description:** Set a Python pretty-printer visualizer for a variable
object. Pass `"None"` to reset. Requires pretty-printing enabled via
`enable_pretty_printing`.

```json
{"name": "var_set_visualizer", "arguments": {"name": "var1", "visualizer": "gdb.default_visualizer"}}
```

**Related:** `enable_pretty_printing`

---

## `enable_pretty_printing`

**MI command:** `-enable-pretty-printing`
**Signature:** `enable_pretty_printing()` — no arguments
**Description:** Enable Python pretty-printing for variable objects
globally. Must be called before varobjs will use registered printers.

```json
{"name": "enable_pretty_printing", "arguments": {}}
```

**Related:** `var_set_visualizer`, `enable_frame_filters`

---

## See also

- `framewalk://guide/variables` — varobj lifecycle and children
- `framewalk://guide/inspection` — when to watch vs. inspect
- `framewalk://reference/data` — one-shot `inspect` and memory
- `framewalk://reference/stack` — locals and arguments at a frame
- `framewalk://reference/breakpoints` — `set_watchpoint` for value change stops
