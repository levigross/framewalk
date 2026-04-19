# File transfer tool reference

Tools for copying files between the host and a connected remote
target. Only meaningful when `target_select` has connected to a
remote that supports the file I/O protocol (most gdbserver and
bare-metal stubs do).

For the broader remote workflow read `framewalk://guide/attach` and
the target tools at `framewalk://reference/target`.

---

## `target_file_put`

**MI command:** `-target-file-put`
**Signature:** `target_file_put(host_file: String, target_file: String)`
**Description:** Copy a file from the host to the remote target.
Useful for uploading test data, libraries, or firmware blobs that
aren't part of the main executable.

```json
{"name": "target_file_put", "arguments": {"host_file": "/tmp/input.bin", "target_file": "/data/input.bin"}}
```

**Related:** `target_file_get`, `target_file_delete`, `target_download`

---

## `target_file_get`

**MI command:** `-target-file-get`
**Signature:** `target_file_get(target_file: String, host_file: String)`
**Description:** Copy a file from the remote target to the host.
Useful for retrieving logs, core dumps, or output files after a run.

```json
{"name": "target_file_get", "arguments": {"target_file": "/data/output.log", "host_file": "/tmp/output.log"}}
```

**Related:** `target_file_put`, `target_file_delete`

---

## `target_file_delete`

**MI command:** `-target-file-delete`
**Signature:** `target_file_delete(target_file: String)`
**Description:** Delete a file on the remote target.

```json
{"name": "target_file_delete", "arguments": {"target_file": "/data/old.log"}}
```

**Related:** `target_file_put`, `target_file_get`

---

## See also

- `framewalk://reference/target` — connecting to the remote
- `framewalk://guide/attach` — remote target workflow
- `framewalk://reference/session` — loading executables on the host side
