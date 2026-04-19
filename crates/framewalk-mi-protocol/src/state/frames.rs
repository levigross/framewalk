//! Frame registry — tracks the current stack frame of each known thread.

use std::collections::BTreeMap;

use framewalk_mi_codec::Value;

use crate::results_view::{get_str, get_u32};
use crate::state::threads::ThreadId;

/// A single stack frame as reported by GDB.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Frame {
    pub level: Option<u32>,
    pub func: Option<String>,
    pub file: Option<String>,
    pub fullname: Option<String>,
    pub line: Option<u32>,
    pub addr: Option<String>,
}

impl Frame {
    #[must_use]
    pub fn from_results(results: &[(String, Value)]) -> Self {
        Self {
            level: get_u32(results, "level"),
            func: get_str(results, "func").map(str::to_owned),
            file: get_str(results, "file").map(str::to_owned),
            fullname: get_str(results, "fullname").map(str::to_owned),
            line: get_u32(results, "line"),
            addr: get_str(results, "addr").map(str::to_owned),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct FrameRegistry {
    by_thread: BTreeMap<ThreadId, Frame>,
}

impl FrameRegistry {
    #[must_use]
    pub fn current(&self, thread: &ThreadId) -> Option<&Frame> {
        self.by_thread.get(thread)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&ThreadId, &Frame)> {
        self.by_thread.iter()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.by_thread.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_thread.is_empty()
    }

    pub(crate) fn on_stopped_frame(&mut self, thread: ThreadId, results: &[(String, Value)]) {
        let Some(frame_results) = results.iter().find_map(|(k, v)| match (k.as_str(), v) {
            ("frame", Value::Tuple(pairs)) => Some(pairs.as_slice()),
            _ => None,
        }) else {
            return;
        };
        self.by_thread
            .insert(thread, Frame::from_results(frame_results));
    }

    pub(crate) fn invalidate(&mut self, thread: &ThreadId) {
        self.by_thread.remove(thread);
    }

    pub(crate) fn clear(&mut self) {
        self.by_thread.clear();
    }
}
