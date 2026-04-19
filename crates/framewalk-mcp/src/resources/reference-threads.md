# Thread tool reference

Tools for listing threads and switching between them. GDB maintains a
"current thread" that all per-thread tools (stack, variables,
execution) operate on — use `select_thread` before `backtrace` /
`inspect` to target a specific thread.

For the conceptual model of inferiors, thread groups, and thread ids
read `framewalk://guide/inspection`.

---

## `list_threads`

**MI command:** `-thread-info`
**Signature:** `list_threads(thread_id: Option<String>)`
**Description:** List threads in the target inferior. Pass a thread id
to query a single thread, or omit to list all. Each entry includes the
thread id, core, state (`running` / `stopped`), and a summary frame.

```json
{"name": "list_threads", "arguments": {}}
```

**Related:** `select_thread`, `thread_list_ids`, `list_thread_groups`

---

## `select_thread`

**MI command:** `-thread-select`
**Signature:** `select_thread(thread_id: String)`
**Description:** Select a thread by id. Subsequent per-thread commands
(`backtrace`, `list_locals`, `step`, etc.) run against the newly
selected thread.

```json
{"name": "select_thread", "arguments": {"thread_id": "2"}}
```

**Related:** `list_threads`, `select_frame`

---

## `thread_list_ids`

**MI command:** `-thread-list-ids`
**Signature:** `thread_list_ids()` — no arguments
**Description:** List thread ids in the target. Cheaper than
`list_threads` when you only need the id list (e.g. to iterate).

```json
{"name": "thread_list_ids", "arguments": {}}
```

**Related:** `list_threads`, `select_thread`

---

## `list_thread_groups`

**MI command:** `-list-thread-groups`
**Signature:** `list_thread_groups(available: bool, recurse: bool, groups: Vec<String>)`
**Description:** List thread groups (inferiors) on the target. Pass
`available: true` to include unattached groups the OS reports (for
attach-by-pid workflows). `recurse: true` descends into child groups.

```json
{"name": "list_thread_groups", "arguments": {"available": false, "recurse": true, "groups": []}}
```

**Related:** `attach`, `framewalk://recipe/attach-running`

---

## See also

- `framewalk://guide/inspection` — threads, frames, and scope
- `framewalk://reference/stack` — operating on the selected thread
- `framewalk://reference/execution` — stepping per thread
- `framewalk://recipe/attach-running` — attaching by pid
