//! MCP resources: instructional content exposed over `resources/list` and
//! `resources/read`.
//!
//! The server ships a fixed catalog of instructional resources — topic guides, per-category tool
//! references, and end-to-end workflow recipes — that an LLM client reads
//! on demand to learn how to use framewalk. This keeps the initial
//! `instructions` string small while making detailed guidance available
//! when it's actually needed.
//!
//! All content is static markdown embedded via `include_str!`, so the
//! resource layer has no runtime state, no subscriptions, and no GDB
//! dependency. Most content is static markdown embedded via `include_str!`;
//! a small number of resources can be generated from canonical Rust data
//! when drift resistance matters more than a checked-in file.

use std::borrow::Cow;

use rmcp::{
    model::{
        AnnotateAble, ListResourcesResult, RawResource, ReadResourceResult, Resource,
        ResourceContents,
    },
    ErrorData as McpError,
};

use crate::raw_guard::{allowed_command_allowlist, AllowlistMatch, ALLOWED_MI_REFERENCE_URI};

/// MIME type reported for every resource.
const MIME: &str = "text/markdown";

/// Resource body source.
enum EntryContent {
    Static(&'static str),
    Generated(fn() -> String),
}

impl EntryContent {
    fn render(&self) -> Cow<'static, str> {
        match self {
            Self::Static(text) => Cow::Borrowed(text),
            Self::Generated(f) => Cow::Owned(f()),
        }
    }
}

/// A single instructional resource: a fixed URI, a short name and
/// description, and its embedded markdown body.
struct Entry {
    uri: &'static str,
    name: &'static str,
    description: &'static str,
    content: EntryContent,
}

fn render_allowed_mi_reference() -> String {
    let mut prefix_entries = Vec::new();
    let mut exact_entries = Vec::new();

    for entry in allowed_command_allowlist() {
        match entry.match_kind {
            AllowlistMatch::Prefix => prefix_entries.push(entry.operation),
            AllowlistMatch::Exact => exact_entries.push(entry.operation),
        }
    }

    let mut out = String::from(
        "# Allowed raw-MI commands\n\n\
The raw-MI guard is allowlist-driven. This resource is generated from \
`raw_guard.rs`, so it is the canonical discoverability surface for what \
`mi_raw_command` and Scheme `(mi ...)` accept when `--allow-shell` is off.\n\n\
## Matching rules\n\n\
- Inputs must still be valid MI commands: they must start with `-` followed by an ASCII letter.\n\
- Prefix entries allow whole MI command families such as `break-*`.\n\
- Exact entries allow only the listed operation, not nearby names that merely share a prefix.\n\
- Anything else is rejected unless framewalk-mcp was started with `--allow-shell`.\n\n\
## Allowed prefix families\n\n",
    );

    for operation in prefix_entries {
        out.push_str("- `");
        out.push_str(operation);
        out.push_str("*`\n");
    }

    out.push_str("\n## Allowed exact commands\n\n");

    for operation in exact_entries {
        out.push_str("- `");
        out.push_str(operation);
        out.push_str("`\n");
    }

    out.push_str(
        "\n## Commonly surprising rejections\n\n\
- `-interpreter-exec ...`\n\
- `-target-exec-command ...`\n\
- raw CLI forms like `shell ls` and `!ls`\n\
- unknown MI families that framewalk has not explicitly allowlisted\n\n\
## See also\n\n\
- `framewalk://reference/raw` — `mi_raw_command` semantics and security model\n\
- `framewalk://reference/support` — `info_mi_command` for probing GDB support\n",
    );

    out
}

/// The full resource registry.
///
/// Paths in `include_str!` are relative to this source file,
/// `crates/framewalk-mcp/src/resources.rs`. So:
///   * `"../../../docs/foo.md"`  → `docs/foo.md` at the repo root
///   * `"resources/foo.md"`      → `crates/framewalk-mcp/src/resources/foo.md`
///
/// A missing target file fails the build, so the registry cannot drift
/// out of sync with on-disk content.
const RESOURCES: &[Entry] = &[
    // ---------------------------------------------------------------
    // Guides — conceptual how-to
    // ---------------------------------------------------------------
    Entry {
        uri: "framewalk://guide/getting-started",
        name: "getting-started",
        description: "First steps: load a binary, set a breakpoint, run, inspect.",
        content: EntryContent::Static(include_str!("../../../docs/getting-started.md")),
    },
    Entry {
        uri: "framewalk://guide/modes",
        name: "modes",
        description: "Full, core, and scheme modes — when to use each.",
        content: EntryContent::Static(include_str!("../../../docs/modes.md")),
    },
    Entry {
        uri: "framewalk://guide/scheme",
        name: "scheme",
        description: "Scheme mode: scheme_eval, the prelude, and composition patterns.",
        content: EntryContent::Static(include_str!("../../../docs/scheme-reference.md")),
    },
    Entry {
        uri: "framewalk://guide/execution-model",
        name: "execution-model",
        description:
            "How MI commands complete, when stops occur, and how to observe local target state.",
        content: EntryContent::Static(include_str!("resources/guide-execution-model.md")),
    },
    Entry {
        uri: "framewalk://guide/breakpoints",
        name: "breakpoints",
        description: "Setting, listing, and modifying breakpoints and watchpoints.",
        content: EntryContent::Static(include_str!("resources/guide-breakpoints.md")),
    },
    Entry {
        uri: "framewalk://guide/execution",
        name: "execution",
        description: "Advanced execution control: step, next, finish, until, jump, reverse.",
        content: EntryContent::Static(include_str!("resources/guide-execution.md")),
    },
    Entry {
        uri: "framewalk://guide/inspection",
        name: "inspection",
        description: "Examining frames, locals, registers, memory, and disassembly at a stop.",
        content: EntryContent::Static(include_str!("resources/guide-inspection.md")),
    },
    Entry {
        uri: "framewalk://guide/variables",
        name: "variables",
        description: "GDB variable objects (watches): create, update, inspect, tear down.",
        content: EntryContent::Static(include_str!("resources/guide-variables.md")),
    },
    Entry {
        uri: "framewalk://guide/tracepoints",
        name: "tracepoints",
        description: "Tracepoints: non-stopping collection for production-safe debugging.",
        content: EntryContent::Static(include_str!("resources/guide-tracepoints.md")),
    },
    Entry {
        uri: "framewalk://guide/raw-mi",
        name: "raw-mi",
        description: "The `mi_raw_command` escape hatch and its shell-guard security model.",
        content: EntryContent::Static(include_str!("resources/guide-raw-mi.md")),
    },
    Entry {
        uri: "framewalk://guide/attach",
        name: "attach",
        description: "Attaching to an already-running process and detaching cleanly.",
        content: EntryContent::Static(include_str!("resources/guide-attach.md")),
    },
    Entry {
        uri: "framewalk://guide/no-source",
        name: "no-source",
        description: "Debugging without source access or with a stripped binary.",
        content: EntryContent::Static(include_str!("../../../docs/no-source.md")),
    },
    // ---------------------------------------------------------------
    // Reference — per-category tool catalogs
    // ---------------------------------------------------------------
    Entry {
        uri: "framewalk://reference/session",
        name: "reference-session",
        description:
            "Session and recovery tools: load_file, attach, target_state, drain_events, reconnect.",
        content: EntryContent::Static(include_str!("resources/reference-session.md")),
    },
    Entry {
        uri: "framewalk://reference/execution",
        name: "reference-execution",
        description: "Execution control tools: run, cont, step, next, finish, until, reverse.",
        content: EntryContent::Static(include_str!("resources/reference-execution.md")),
    },
    Entry {
        uri: "framewalk://reference/breakpoints",
        name: "reference-breakpoints",
        description: "Breakpoint and watchpoint tools.",
        content: EntryContent::Static(include_str!("resources/reference-breakpoints.md")),
    },
    Entry {
        uri: "framewalk://reference/catchpoints",
        name: "reference-catchpoints",
        description: "Catchpoints for signals, library events, and exception handling.",
        content: EntryContent::Static(include_str!("resources/reference-catchpoints.md")),
    },
    Entry {
        uri: "framewalk://reference/stack",
        name: "reference-stack",
        description: "Frame, backtrace, locals, and arguments tools.",
        content: EntryContent::Static(include_str!("resources/reference-stack.md")),
    },
    Entry {
        uri: "framewalk://reference/threads",
        name: "reference-threads",
        description: "Thread listing, selection, and thread-group tools.",
        content: EntryContent::Static(include_str!("resources/reference-threads.md")),
    },
    Entry {
        uri: "framewalk://reference/data",
        name: "reference-data",
        description: "Expression evaluation, memory, disassembly, and register tools.",
        content: EntryContent::Static(include_str!("resources/reference-data.md")),
    },
    Entry {
        uri: "framewalk://reference/variables",
        name: "reference-variables",
        description: "Variable-object (watch) lifecycle and inspection tools.",
        content: EntryContent::Static(include_str!("resources/reference-variables.md")),
    },
    Entry {
        uri: "framewalk://reference/symbols",
        name: "reference-symbols",
        description: "Symbol, type, and source-line information tools.",
        content: EntryContent::Static(include_str!("resources/reference-symbols.md")),
    },
    Entry {
        uri: "framewalk://reference/tracepoints",
        name: "reference-tracepoints",
        description: "Tracepoint definition, collection, and frame navigation tools.",
        content: EntryContent::Static(include_str!("resources/reference-tracepoints.md")),
    },
    Entry {
        uri: "framewalk://reference/target",
        name: "reference-target",
        description: "Target-connection tools: target_select, download, disconnect.",
        content: EntryContent::Static(include_str!("resources/reference-target.md")),
    },
    Entry {
        uri: "framewalk://reference/file-transfer",
        name: "reference-file-transfer",
        description: "Remote-target file transfer tools.",
        content: EntryContent::Static(include_str!("resources/reference-file-transfer.md")),
    },
    Entry {
        uri: "framewalk://reference/support",
        name: "reference-support",
        description: "Feature introspection, gdb-set/show, and mi-command queries.",
        content: EntryContent::Static(include_str!("resources/reference-support.md")),
    },
    Entry {
        uri: ALLOWED_MI_REFERENCE_URI,
        name: "reference-allowed-mi",
        description: "Canonical raw-MI allowlist generated from the guard implementation.",
        content: EntryContent::Generated(render_allowed_mi_reference),
    },
    Entry {
        uri: "framewalk://reference/raw",
        name: "reference-raw",
        description: "The `mi_raw_command` tool and its shell-guard allowlist.",
        content: EntryContent::Static(include_str!("resources/reference-raw.md")),
    },
    Entry {
        uri: "framewalk://reference/scheme",
        name: "reference-scheme",
        description: "The `scheme_eval` tool and the full Scheme prelude function list.",
        content: EntryContent::Static(include_str!("resources/reference-scheme.md")),
    },
    // ---------------------------------------------------------------
    // Recipes — end-to-end workflows
    // ---------------------------------------------------------------
    Entry {
        uri: "framewalk://recipe/debug-segfault",
        name: "debug-segfault",
        description: "Diagnose a segmentation fault: run, backtrace, inspect, walk frames.",
        content: EntryContent::Static(include_str!("resources/recipe-debug-segfault.md")),
    },
    Entry {
        uri: "framewalk://recipe/attach-running",
        name: "attach-running",
        description: "Attach to a running process, inspect it, detach cleanly.",
        content: EntryContent::Static(include_str!("resources/recipe-attach-running.md")),
    },
    Entry {
        uri: "framewalk://recipe/conditional-breakpoint",
        name: "conditional-breakpoint",
        description: "Set a conditional breakpoint and tune its condition and ignore count.",
        content: EntryContent::Static(include_str!("resources/recipe-conditional-breakpoint.md")),
    },
    Entry {
        uri: "framewalk://recipe/tracepoint-session",
        name: "tracepoint-session",
        description: "Full tracepoint workflow: insert, start, navigate frames, save.",
        content: EntryContent::Static(include_str!("resources/recipe-tracepoint-session.md")),
    },
    Entry {
        uri: "framewalk://recipe/kernel-debug",
        name: "kernel-debug",
        description: "Debug an early-boot kernel under QEMU: --no-non-stop, HW breakpoints, warning surfacing.",
        content: EntryContent::Static(include_str!("resources/recipe-kernel-debug.md")),
    },
];

/// Return the full list of available resources for `resources/list`.
///
/// The list is static for a given build — the registry is a `const`
/// table — so this function allocates on each call but performs no I/O.
pub(crate) fn list_resources() -> ListResourcesResult {
    let items: Vec<Resource> = RESOURCES
        .iter()
        .map(|e| {
            let content = e.content.render();
            let mut raw = RawResource::new(e.uri, e.name);
            raw.description = Some(e.description.to_string());
            raw.mime_type = Some(MIME.to_string());
            raw.size = Some(u32::try_from(content.len()).unwrap_or(u32::MAX));
            raw.no_annotation()
        })
        .collect();
    ListResourcesResult::with_all_items(items)
}

/// Look up a resource by URI and return its content.
///
/// Returns `invalid_params` if the URI is not in the registry.
pub(crate) fn read_resource(uri: &str) -> Result<ReadResourceResult, McpError> {
    let entry = RESOURCES
        .iter()
        .find(|e| e.uri == uri)
        .ok_or_else(|| McpError::invalid_params(format!("unknown resource uri: {uri}"), None))?;
    let text = entry.content.render();
    Ok(ReadResourceResult::new(vec![
        ResourceContents::TextResourceContents {
            uri: entry.uri.to_string(),
            mime_type: Some(MIME.to_string()),
            text: text.into_owned(),
            meta: None,
        },
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn registry_count_matches_plan() {
        // 12 guides + 16 references + 5 recipes = 33 total.
        assert_eq!(RESOURCES.len(), 33, "expected exactly 33 resources");
    }

    #[test]
    fn every_uri_is_unique() {
        let mut seen = HashSet::new();
        for e in RESOURCES {
            assert!(seen.insert(e.uri), "duplicate uri in registry: {}", e.uri);
        }
    }

    #[test]
    fn every_uri_uses_framewalk_scheme() {
        for e in RESOURCES {
            assert!(
                e.uri.starts_with("framewalk://"),
                "uri missing framewalk:// scheme: {}",
                e.uri
            );
        }
    }

    #[test]
    fn every_content_is_non_empty() {
        for e in RESOURCES {
            assert!(
                !e.content.render().trim().is_empty(),
                "empty content for resource: {}",
                e.uri
            );
        }
    }

    #[test]
    fn every_description_is_non_empty() {
        for e in RESOURCES {
            assert!(
                !e.description.trim().is_empty(),
                "empty description for resource: {}",
                e.uri
            );
        }
    }

    #[test]
    fn list_resources_reports_markdown_mime_and_size() {
        let list = list_resources();
        assert_eq!(list.resources.len(), RESOURCES.len());
        for r in &list.resources {
            assert_eq!(r.mime_type.as_deref(), Some("text/markdown"));
            assert!(r.size.unwrap_or(0) > 0, "zero size for {}", r.uri);
        }
    }

    #[test]
    fn read_resource_getting_started_matches_docs_file() {
        let result = read_resource("framewalk://guide/getting-started")
            .expect("getting-started should exist");
        let ResourceContents::TextResourceContents {
            text, mime_type, ..
        } = &result.contents[0]
        else {
            panic!("expected TextResourceContents");
        };
        assert_eq!(mime_type.as_deref(), Some("text/markdown"));
        assert_eq!(text, include_str!("../../../docs/getting-started.md"));
    }

    #[test]
    fn read_resource_reference_breakpoints_non_empty() {
        let result = read_resource("framewalk://reference/breakpoints")
            .expect("reference/breakpoints should exist");
        let ResourceContents::TextResourceContents { text, .. } = &result.contents[0] else {
            panic!("expected TextResourceContents");
        };
        assert!(!text.trim().is_empty());
    }

    #[test]
    fn read_unknown_uri_is_invalid_params() {
        let err = read_resource("framewalk://nope").expect_err("should reject unknown uri");
        assert!(
            format!("{err:?}").contains("nope"),
            "error should mention the bad uri, got: {err:?}"
        );
    }

    #[test]
    fn registry_includes_all_three_families() {
        let guides = RESOURCES
            .iter()
            .filter(|e| e.uri.starts_with("framewalk://guide/"))
            .count();
        let refs = RESOURCES
            .iter()
            .filter(|e| e.uri.starts_with("framewalk://reference/"))
            .count();
        let recipes = RESOURCES
            .iter()
            .filter(|e| e.uri.starts_with("framewalk://recipe/"))
            .count();
        assert_eq!(guides, 12, "expected 12 guides");
        assert_eq!(refs, 16, "expected 16 references");
        assert_eq!(recipes, 5, "expected 5 recipes");
    }

    #[test]
    fn generated_allowed_mi_reference_mentions_canonical_entries() {
        let result = read_resource(ALLOWED_MI_REFERENCE_URI)
            .expect("generated allowlist resource should exist");
        let ResourceContents::TextResourceContents { text, .. } = &result.contents[0] else {
            panic!("expected TextResourceContents");
        };
        assert!(
            text.contains("break-*"),
            "allowlist resource should mention prefix entries"
        );
        assert!(
            text.contains("target-select"),
            "allowlist resource should mention exact entries"
        );
    }
}
