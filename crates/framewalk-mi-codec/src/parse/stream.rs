//! Parser for stream records: `("~" | "@" | "&") c-string`.
//!
//! Stream records carry **no token** per the MI BNF (the grammar has no
//! `[token]` prefix on stream productions). The top-level dispatcher in
//! [`parse::parse_record`](crate::parse::parse_record) enforces that a
//! stream record prefix byte is never preceded by a numeric token — if a
//! token is seen before `~`, `@`, or `&`, the caller raises
//! [`CodecErrorKind::StreamRecordHasToken`](
//! crate::error::CodecErrorKind::StreamRecordHasToken).
//!
//! This function only parses the c-string payload after the prefix byte.

use crate::ast::StreamRecord;
use crate::error::CodecError;
use crate::parse::cstring::parse_cstring;
use crate::parse::cursor::Cursor;

/// Parse a stream record's payload. The cursor must be positioned *after*
/// the leading `~`, `@`, or `&` byte.
pub(crate) fn parse_stream_record(cursor: &mut Cursor<'_>) -> Result<StreamRecord, CodecError> {
    let text = parse_cstring(cursor)?;
    Ok(StreamRecord { text })
}
