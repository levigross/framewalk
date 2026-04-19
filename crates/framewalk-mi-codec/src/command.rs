//! The outbound command type.
//!
//! An [`MiCommand`] is what the protocol layer builds and hands to the
//! encoder to serialise into bytes for writing to GDB's stdin. It mirrors
//! the input grammar from the GDB manual:
//!
//! ```text
//! mi-command → [token] "-" operation (" " option)* [" --"] (" " parameter)* nl
//! option     → "-" parameter [" " parameter]
//! parameter  → non-blank-sequence | c-string
//! ```
//!
//! Callers never pick tokens themselves — the protocol layer's `Connection`
//! assigns them internally when `submit` is called — so [`MiCommand`] only
//! carries the operation name, options, and parameters. The token is stapled
//! on at encode time.

use alloc::string::String;
use alloc::vec::Vec;

/// An outbound MI command, prior to encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MiCommand {
    /// The operation name, without the leading `-`. For example
    /// `"exec-run"`, `"break-insert"`, `"var-create"`.
    pub operation: String,

    /// Options preceding the positional parameters. Each option is encoded
    /// as `-name` optionally followed by a value.
    pub options: Vec<CommandOption>,

    /// Positional parameters after the options. When the first parameter
    /// starts with `-`, the encoder emits a `--` separator before it so
    /// GDB doesn't misinterpret it as an option.
    pub parameters: Vec<String>,
}

impl MiCommand {
    /// Create a command with the given operation name and no options or
    /// parameters. Use the builder-style [`option`](Self::option) and
    /// [`parameter`](Self::parameter) methods to add more.
    #[must_use]
    pub fn new(operation: impl Into<String>) -> Self {
        Self {
            operation: operation.into(),
            options: Vec::new(),
            parameters: Vec::new(),
        }
    }

    /// Append an option with no value.
    #[must_use]
    pub fn option(mut self, name: impl Into<String>) -> Self {
        self.options.push(CommandOption {
            name: name.into(),
            value: None,
        });
        self
    }

    /// Append an option with a string value.
    #[must_use]
    pub fn option_with(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.options.push(CommandOption {
            name: name.into(),
            value: Some(value.into()),
        });
        self
    }

    /// Append a positional parameter.
    #[must_use]
    pub fn parameter(mut self, value: impl Into<String>) -> Self {
        self.parameters.push(value.into());
        self
    }
}

/// A single option on an [`MiCommand`].
///
/// `name` is the option name without the leading `-`. `value` is an optional
/// string argument to the option. For example `CommandOption { name: "t",
/// value: None }` encodes as `-t`, and `CommandOption { name: "condition",
/// value: Some("x > 5") }` encodes as `-condition "x > 5"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOption {
    pub name: String,
    pub value: Option<String>,
}
