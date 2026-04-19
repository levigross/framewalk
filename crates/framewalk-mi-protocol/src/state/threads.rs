//! Thread registry and thread-id newtypes.
//!
//! `ThreadId`, `ThreadGroupId`, and `InferiorId` are deliberately distinct
//! newtypes so the four GDB identifier spaces (thread id, thread-group id,
//! inferior number, OS pid) never leak as bare strings in public API. That
//! separation is enforced by the type system so callers cannot conflate
//! them — a known foot-gun across every MI frontend.

use std::collections::BTreeMap;

use framewalk_mi_codec::Value;

use crate::results_view::get_str;

/// A thread identifier, as GDB uses it in `*stopped,thread-id="1"` and
/// similar. GDB may also report `"all"` here in all-stop mode when an
/// event applies to every thread; callers handle that explicitly via
/// [`ThreadId::is_all`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ThreadId(String);

impl ThreadId {
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// `true` if this is the pseudo-id `"all"` that GDB uses when an
    /// exec event applies to every thread.
    #[must_use]
    pub fn is_all(&self) -> bool {
        self.0 == "all"
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Display for ThreadId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A thread-group identifier, as GDB uses it for process/inferior groups.
/// Examples: `"i1"`, `"i2"`. Distinct type from [`ThreadId`] — do not
/// conflate.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ThreadGroupId(String);

impl ThreadGroupId {
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Execution state of a single thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadState {
    Running,
    Stopped,
}

/// Per-thread metadata tracked by the protocol layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadInfo {
    pub id: ThreadId,
    pub group_id: Option<ThreadGroupId>,
    /// Last observed execution state, or `None` if never reported.
    pub state: Option<ThreadState>,
    /// Human-readable thread name if GDB supplied one.
    pub name: Option<String>,
}

impl ThreadInfo {
    fn new(id: ThreadId) -> Self {
        Self {
            id,
            group_id: None,
            state: None,
            name: None,
        }
    }
}

/// Registry of known threads. Keyed by [`ThreadId`] and ordered for
/// deterministic iteration in tool output.
#[derive(Debug, Clone, Default)]
pub struct ThreadRegistry {
    by_id: BTreeMap<ThreadId, ThreadInfo>,
}

impl ThreadRegistry {
    /// Iterate threads in ascending id order.
    pub fn iter(&self) -> impl Iterator<Item = (&ThreadId, &ThreadInfo)> {
        self.by_id.iter()
    }

    /// Number of known threads.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }

    /// Look up one thread by id.
    #[must_use]
    pub fn get(&self, id: &ThreadId) -> Option<&ThreadInfo> {
        self.by_id.get(id)
    }

    // ---- Mutators driven by Connection event routing ----

    /// Handle `=thread-created,id=...,group-id=...`.
    pub(crate) fn on_thread_created(&mut self, results: &[(String, Value)]) {
        let Some(id) = get_str(results, "id") else {
            return;
        };
        let thread_id = ThreadId::new(id);
        let group_id = get_str(results, "group-id").map(ThreadGroupId::new);
        self.by_id
            .entry(thread_id.clone())
            .and_modify(|info| {
                if info.group_id.is_none() {
                    info.group_id.clone_from(&group_id);
                }
            })
            .or_insert_with(|| ThreadInfo {
                id: thread_id,
                group_id,
                state: None,
                name: None,
            });
    }

    /// Handle `=thread-exited,id=...`.
    pub(crate) fn on_thread_exited(&mut self, results: &[(String, Value)]) {
        let Some(id) = get_str(results, "id") else {
            return;
        };
        self.by_id.remove(&ThreadId::new(id));
    }

    /// Record a `*running,thread-id=...` transition. If `thread-id` is
    /// `"all"`, every known thread is marked running.
    pub(crate) fn on_running(&mut self, thread_id: Option<&str>) {
        match thread_id {
            Some("all") | None => {
                for info in self.by_id.values_mut() {
                    info.state = Some(ThreadState::Running);
                }
            }
            Some(id) => {
                let key = ThreadId::new(id);
                self.by_id
                    .entry(key.clone())
                    .or_insert_with(|| ThreadInfo::new(key))
                    .state = Some(ThreadState::Running);
            }
        }
    }

    /// Record a `*stopped,thread-id=...` transition.
    pub(crate) fn on_stopped(&mut self, thread_id: Option<&str>) {
        match thread_id {
            Some("all") | None => {
                for info in self.by_id.values_mut() {
                    info.state = Some(ThreadState::Stopped);
                }
            }
            Some(id) => {
                let key = ThreadId::new(id);
                self.by_id
                    .entry(key.clone())
                    .or_insert_with(|| ThreadInfo::new(key))
                    .state = Some(ThreadState::Stopped);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn results(pairs: &[(&str, &str)]) -> Vec<(String, Value)> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), Value::Const((*v).to_string())))
            .collect()
    }

    // ---- ThreadId / ThreadGroupId ----

    #[test]
    fn thread_id_all_sentinel() {
        assert!(ThreadId::new("all").is_all());
        assert!(!ThreadId::new("1").is_all());
    }

    #[test]
    fn thread_id_display_matches_as_str() {
        let id = ThreadId::new("42");
        assert_eq!(id.as_str(), "42");
        assert_eq!(format!("{id}"), "42");
    }

    #[test]
    fn thread_ids_are_distinct_across_spaces() {
        // ThreadId and ThreadGroupId are separate types so callers cannot
        // mix them up. We cannot assert "compile error" in a runtime test,
        // but we can assert that both round-trip through their as_str.
        let tid = ThreadId::new("1");
        let gid = ThreadGroupId::new("i1");
        assert_eq!(tid.as_str(), "1");
        assert_eq!(gid.as_str(), "i1");
    }

    // ---- Registry ----

    #[test]
    fn empty_registry() {
        let r = ThreadRegistry::default();
        assert!(r.is_empty());
        assert_eq!(r.len(), 0);
        assert!(r.get(&ThreadId::new("1")).is_none());
    }

    #[test]
    fn thread_created_inserts_info_with_group() {
        let mut r = ThreadRegistry::default();
        r.on_thread_created(&results(&[("id", "1"), ("group-id", "i1")]));
        let info = r.get(&ThreadId::new("1")).expect("thread 1 inserted");
        assert_eq!(info.id.as_str(), "1");
        assert_eq!(info.group_id.as_ref().unwrap().as_str(), "i1");
        assert_eq!(info.state, None);
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn thread_created_missing_id_is_noop() {
        let mut r = ThreadRegistry::default();
        r.on_thread_created(&results(&[("group-id", "i1")]));
        assert!(r.is_empty());
    }

    #[test]
    fn thread_created_populates_group_on_repeat_without_overwriting() {
        let mut r = ThreadRegistry::default();
        // First observation: no group-id known.
        r.on_thread_created(&results(&[("id", "1")]));
        let first_group = r.get(&ThreadId::new("1")).unwrap().group_id.clone();
        assert!(first_group.is_none());

        // Second observation: group-id now supplied, should backfill.
        r.on_thread_created(&results(&[("id", "1"), ("group-id", "i1")]));
        assert_eq!(
            r.get(&ThreadId::new("1"))
                .unwrap()
                .group_id
                .as_ref()
                .unwrap()
                .as_str(),
            "i1"
        );
    }

    #[test]
    fn thread_exited_removes() {
        let mut r = ThreadRegistry::default();
        r.on_thread_created(&results(&[("id", "1"), ("group-id", "i1")]));
        r.on_thread_exited(&results(&[("id", "1")]));
        assert!(r.is_empty());
    }

    #[test]
    fn thread_exited_missing_id_is_noop() {
        let mut r = ThreadRegistry::default();
        r.on_thread_created(&results(&[("id", "1")]));
        r.on_thread_exited(&results(&[("other", "x")]));
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn on_running_for_all_updates_every_known_thread() {
        let mut r = ThreadRegistry::default();
        r.on_thread_created(&results(&[("id", "1")]));
        r.on_thread_created(&results(&[("id", "2")]));
        r.on_running(Some("all"));
        for (_, info) in r.iter() {
            assert_eq!(info.state, Some(ThreadState::Running));
        }
    }

    #[test]
    fn on_running_none_equivalent_to_all() {
        let mut r = ThreadRegistry::default();
        r.on_thread_created(&results(&[("id", "1")]));
        r.on_running(None);
        assert_eq!(
            r.get(&ThreadId::new("1")).unwrap().state,
            Some(ThreadState::Running)
        );
    }

    #[test]
    fn on_running_specific_thread_creates_if_absent() {
        let mut r = ThreadRegistry::default();
        r.on_running(Some("3"));
        let info = r.get(&ThreadId::new("3")).expect("thread 3 auto-created");
        assert_eq!(info.state, Some(ThreadState::Running));
    }

    #[test]
    fn on_stopped_flips_state() {
        let mut r = ThreadRegistry::default();
        r.on_thread_created(&results(&[("id", "1")]));
        r.on_running(Some("1"));
        assert_eq!(
            r.get(&ThreadId::new("1")).unwrap().state,
            Some(ThreadState::Running)
        );
        r.on_stopped(Some("1"));
        assert_eq!(
            r.get(&ThreadId::new("1")).unwrap().state,
            Some(ThreadState::Stopped)
        );
    }

    #[test]
    fn on_stopped_all_flips_every_thread() {
        let mut r = ThreadRegistry::default();
        r.on_thread_created(&results(&[("id", "1")]));
        r.on_thread_created(&results(&[("id", "2")]));
        r.on_running(Some("all"));
        r.on_stopped(Some("all"));
        for (_, info) in r.iter() {
            assert_eq!(info.state, Some(ThreadState::Stopped));
        }
    }

    #[test]
    fn iter_is_ordered_by_thread_id() {
        let mut r = ThreadRegistry::default();
        r.on_thread_created(&results(&[("id", "3")]));
        r.on_thread_created(&results(&[("id", "1")]));
        r.on_thread_created(&results(&[("id", "2")]));
        let ids: Vec<_> = r.iter().map(|(id, _)| id.as_str().to_owned()).collect();
        assert_eq!(ids, vec!["1", "2", "3"]);
    }
}
