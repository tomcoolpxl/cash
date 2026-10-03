//! Error facilities

use std::path::PathBuf;

use crate::{Shell, ShellFd, extensions, results, sys};

/// Unified error type for this crate. Contains just a kind for now,
/// but will be extended later with additional context.
#[derive(thiserror::Error, Debug)]
#[error("{kind}")]
pub struct Error {
    /// The kind of error.
    #[source]
    kind: ErrorKind,

    /// Whether or not the error should be considered a "fatal" error that would
    /// result in abnormal exit of a non-interactive shell.
    fatal: bool,

    /// Where it happened, as the start of its message (`lib.sh: line 3: `), when it
    /// left the function or file it happened in before it was shown.
    location: Option<String>,
}

/// Monolithic error type for the shell
#[derive(thiserror::Error, Debug)]
pub enum ErrorKind {
    /// A tilde expression was used without a valid HOME variable
    #[error("cannot expand tilde expression with HOME not set")]
    TildeWithoutValidHome,

    /// An attempt was made to assign a list to an array member, named as `a[0]`.
    #[error("{0}: cannot assign list to array member")]
    AssigningListToArrayMember(String),

    /// An attempt was made to convert an associative array to an indexed array, named
    /// where the caller knows it ([`Error::of_variable`]).
    #[error("{}cannot convert associative to indexed array", named(.0))]
    ConvertingAssociativeArrayToIndexedArray(String),

    /// An attempt was made to convert an indexed array to an associative array.
    #[error("{}cannot convert indexed to associative array", named(.0))]
    ConvertingIndexedArrayToAssociativeArray(String),

    /// An error occurred while sourcing the indicated script file.
    #[error("{}: {}", .0.display(), os_error_text(.1))]
    FailedSourcingFile(PathBuf, #[source] std::io::Error),

    /// A script file given to the shell looks like a binary file.
    #[error("{}: cannot execute binary file", .0.display())]
    CannotExecuteBinaryFile(PathBuf),

    /// The process or process group does not exist.
    #[error("no such process")]
    NoSuchProcess,

    /// The process or process group exists, but cannot be signaled.
    #[error("operation not permitted")]
    PermissionDenied,

    /// The shell failed to send a signal to a process.
    #[error("failed to send signal to process")]
    FailedToSendSignal,

    /// An attempt was made to assign a value to a special parameter, named as `$1`.
    #[error("{0}: cannot assign in this way")]
    CannotAssignToSpecialParameter(String),

    /// Checked expansion error.
    #[error("{0}")]
    CheckedExpansionError(String),

    /// A reference was made to an unknown shell function.
    #[error("function not found: {0}")]
    FunctionNotFound(String),

    /// Command was not found.
    #[error("{}", command_not_found(.0))]
    CommandNotFound(String),

    /// Not a builtin.
    #[error("not a shell builtin: {0}")]
    BuiltinNotFound(String),

    /// The working directory does not exist.
    #[error("working directory does not exist: {0}")]
    WorkingDirMissing(PathBuf),

    /// Failed to execute command.
    #[error("{}: {}", .0, os_error_text(.1))]
    FailedToExecuteCommand(String, #[source] std::io::Error),

    /// History item was not found.
    #[error("history item not found")]
    HistoryItemNotFound,

    /// The requested functionality has not yet been implemented in this shell.
    #[error("not yet implemented: {0}")]
    Unimplemented(&'static str),

    /// The requested functionality has not yet been implemented in this shell; it is tracked in a
    /// GitHub issue.
    #[error("not yet implemented: {0}; see https://github.com/thraa/cash/issues/{1}")]
    UnimplementedAndTracked(&'static str, u32),

    /// An expected environment scope could not be found.
    #[error("missing environment scope")]
    MissingScope,

    /// The environment scope required for a new variable is not available.
    #[error("environment scope required for new variable is not available")]
    MissingScopeForNewVariable,

    /// An unexpected environment scope type was encountered.
    #[error("unexpected environment scope type: expected '{expected}', found '{actual}'")]
    UnexpectedScopeType {
        /// The expected scope type.
        expected: crate::env::EnvironmentScope,
        /// The actual scope type.
        actual: crate::env::EnvironmentScope,
    },

    /// The given path is not a directory.
    #[error("{}: Not a directory", .0.display())]
    NotADirectory(PathBuf),

    /// The given path is a directory.
    #[error("path is a directory")]
    IsADirectory,

    /// The given variable is not an array.
    #[error("variable is not an array")]
    NotArray,

    /// The current user could not be determined.
    #[error("no current user")]
    NoCurrentUser,

    /// The requested input or output redirection is invalid.
    #[error("invalid redirection target")]
    InvalidRedirection,

    /// An error occurred while redirecting input or output with the given file.
    #[error("{0}: {1}")]
    RedirectionFailure(String, String),

    /// An error occurred evaluating an arithmetic expression.
    #[error("{0}")]
    EvalError(#[from] crate::arithmetic::EvalError),

    /// The given string could not be parsed as an integer.
    #[error("failed to parse '{s}' as a {int_type_name}, base-{radix} integer: {inner}")]
    IntParseError {
        /// The string that failed to parse.
        s: String,
        /// The integer type being parsed.
        int_type_name: &'static str,
        /// The radix (base) used for parsing.
        radix: u32,
        /// The underlying parse error.
        inner: std::num::ParseIntError,
    },

    /// The given integer could not be converted to the target type.
    #[error("integer conversion error")]
    TryIntParseError(#[from] std::num::TryFromIntError),

    /// A byte sequence could not be decoded as a valid UTF-8 string.
    #[error("failed to decode utf-8")]
    FromUtf8Error(#[from] std::string::FromUtf8Error),

    /// A byte sequence could not be decoded as a valid UTF-8 string.
    #[error("failed to decode utf-8")]
    Utf8Error(#[from] std::str::Utf8Error),

    /// An attempt was made to modify a readonly variable, named where the error is
    /// known to be about one ([`Error::of_variable`]).
    #[error("{}", if .0.is_empty() { "readonly variable".to_owned() } else { format!("{}: readonly variable", .0) })]
    ReadonlyVariable(String),

    /// An assignment through a chain of name references that comes back to itself.
    #[error("warning: {0}: circular name reference")]
    CircularNameReference(String),

    /// The indicated pattern is invalid.
    #[error("invalid pattern: '{0}'")]
    InvalidPattern(String),

    /// A regular expression error occurred
    #[error("regex error: {0}")]
    RegexError(#[from] fancy_regex::Error),

    /// An invalid regular expression was provided.
    #[error("invalid regex: {0}; expression: '{1}'")]
    InvalidRegexError(fancy_regex::Error, String),

    /// An I/O error occurred.
    #[error("i/o error: {0}")]
    IoError(#[from] std::io::Error),

    /// Invalid substitution syntax.
    #[error("bad substitution: {0}")]
    BadSubstitution(String),

    /// A parameter transformation used an unknown or missing transformation operator.
    /// Bash uses status 127 when this fatal expansion aborts a non-interactive shell,
    /// while an interactive shell recovers with status 1.
    #[error("bad substitution: {0}")]
    InvalidParameterTransformation(String),

    /// A `${…}` that is no parameter expansion, worded as Bash words it:
    /// `${x y}: bad substitution`, or for an unclosed subscript ``bad substitution: no
    /// closing `}' in "${a[}"``. It abandons the command it is in, as in Bash.
    #[error("{0}")]
    BadSubstitutionText(String),

    /// An error occurred while creating a child process.
    #[error("failed to create child process")]
    ChildCreationFailure,

    /// An error occurred while formatting a string.
    #[error(transparent)]
    FormattingError(#[from] std::fmt::Error),

    /// An error occurred while parsing.
    #[error("{0}")]
    ParseError(crate::parser::ParseError, crate::SourceInfo),

    /// An error occurred while parsing a function body.
    #[error("{0}: {1}")]
    FunctionParseError(String, crate::parser::ParseError),

    /// An error occurred while parsing a word.
    #[error(transparent)]
    WordParseError(#[from] crate::parser::WordParseError),

    /// Unable to parse a test command.
    #[error("invalid test command")]
    TestCommandParseError(#[from] crate::parser::TestCommandParseError),

    /// Unable to parse a key binding specification.
    #[error(transparent)]
    BindingParseError(#[from] crate::parser::BindingParseError),

    /// A threading error occurred.
    #[error("threading error")]
    ThreadingError(#[from] tokio::task::JoinError),

    /// An invalid signal was referenced.
    #[error("{0}: invalid signal specification")]
    InvalidSignal(String),

    /// A platform error occurred.
    #[error("platform error: {0}")]
    PlatformError(#[from] sys::PlatformError),

    /// An invalid umask was provided.
    #[error("{0}")]
    InvalidUmask(String),

    /// A descriptor `read -u` or `mapfile -u` was given that is not open.
    #[error("{0}: invalid file descriptor: Bad file descriptor")]
    InvalidFileDescriptor(ShellFd),

    /// A file or folder a builtin could not use, as Bash's `file_error` names it:
    /// `/no/such: No such file or directory`.
    #[error("{}: {}", .0, os_error_text(.1))]
    FileError(String, std::io::Error),

    /// An error in the assignment a declaration builtin makes (`declare -a d=(1 2)` of an
    /// associative `d`), as Bash reports one: without the builtin's name, abandoning the
    /// command line.
    #[error("{0}")]
    AssignmentError(Box<Error>),

    /// `test` or `[` refused its arguments, worded as Bash words it: `x: integer
    /// expected`, `1: unary operator expected`, `too many arguments`. Status 2.
    #[error("{0}")]
    TestError(String),

    /// `unset` of a readonly variable.
    #[error("{0}: cannot unset: readonly variable")]
    CannotUnsetReadonly(String),

    /// The given open file cannot be read from.
    #[error("cannot read from {0}")]
    OpenFileNotReadable(&'static str),

    /// The given open file cannot be written to.
    #[error("cannot write to {0}")]
    OpenFileNotWritable(&'static str),

    /// Bad file descriptor.
    #[error("{0}: Bad file descriptor")]
    BadFileDescriptor(ShellFd),

    /// Printf failure
    #[error("printf failure: {0}")]
    PrintfFailure(i32),

    /// Printf invalid usage
    #[error("printf: {0}")]
    PrintfInvalidUsage(String),

    /// Interrupted
    #[error("interrupted")]
    Interrupted,

    /// A call to the named function would go past the nesting limit (`FUNCNEST`, or 500).
    #[error("{0}: maximum function nesting level exceeded ({1})")]
    MaxFunctionCallDepthExceeded(String, usize),

    /// Fork resource temporarily unavailable (process / subshell limit reached).
    #[error("fork: retry: Resource temporarily unavailable")]
    ForkResourceUnavailable,

    /// System time error.
    #[error("system time error: {0}")]
    TimeError(#[from] std::time::SystemTimeError),

    /// Array index out of range.
    #[error("{0}: bad array subscript")]
    ArrayIndexOutOfRange(String),

    /// A command substitution in a subscript that a builtin would expand a second time.
    #[error("{0}: command substitution in a subscript is not run (cash)")]
    CommandSubstitutionInSubscript(String),

    /// Unhandled key code.
    #[error("unhandled key code: {0:?}")]
    UnhandledKeyCode(Vec<u8>),

    /// An error occurred in a built-in command.
    #[error("{}", if .0.names_its_builtin() { format!("{}: {}", .1, .0) } else { .0.to_string() })]
    BuiltinError(Box<dyn BuiltinError>, String),

    /// Operation not supported on this platform.
    #[error("operation not supported on this platform: {0}")]
    NotSupportedOnThisPlatform(&'static str),

    /// Command history is not enabled in this shell.
    #[error("command history is not enabled in this shell")]
    HistoryNotEnabled,

    /// Expanding an unset variable, named as Bash names it (`u`, `a[3]`, `$1`).
    #[error("{0}: unbound variable")]
    ExpandingUnsetVariable(String),

    /// An internal error occurred.
    #[error("internal shell error: {0}")]
    InternalError(String),

    /// Attempted to perform an operation that requires an interactive session.
    #[error("operation requires an interactive session")]
    NotInInteractiveSession,

    /// Attempted to perform an operation that requires command-string mode.
    #[error("operation requires command-string mode")]
    NotExecutingCommandString,

    /// Too much data was provided to an operation.
    #[error("too much data")]
    TooMuchData,

    /// Cannot convert open file to native file descriptor.
    #[error("cannot convert open file to native file descriptor")]
    CannotConvertToNativeFd,

    /// History file is too large to import.
    #[error("history file is too large to import")]
    HistoryFileTooLargeToImport,

    /// Too many open files.
    #[error("too many open files")]
    TooManyOpenFiles,

    /// The function name shadows a special built-in command.
    #[error("function name '{}' shadows a special built-in command", .name)]
    FunctionNameShadowsSpecialBuiltin {
        /// Name of the function.
        name: String,
    },

    /// A glob pattern failed to match any files (failglob).
    #[error("no match: {0}")]
    NoMatch(String),

    /// A `#!/usr/bin/env` line `env` refuses, worded as `env` words it; status 125, as
    /// GNU `env` exits (W32-08).
    #[error("{0}: {1}")]
    EnvShebang(String, String),
}

/// Trait implementable by built-in commands to represent errors.
pub trait BuiltinError: std::error::Error + ConvertibleToExitCode + Send + Sync {
    /// Try to extract a reference to the underlying `std::io::Error`, if any.
    /// Implementations should return `None` if there is no inner I/O error.
    /// They should not attempt to *synthesize* an I/O error if one does not
    /// naturally exist.
    fn as_io_error(&self) -> Option<&std::io::Error> {
        None
    }

    /// Whether this is an arithmetic evaluation error, which abandons the command line
    /// it happened on (see [`Error::discards_line`]).
    fn is_arithmetic_error(&self) -> bool {
        false
    }

    /// Whether this is an interrupt no trap is set for: Ctrl-C while a builtin waited for
    /// the keyboard, or an `INT` the interactive shell sent itself. It abandons the
    /// command line at the prompt, and ends a script (see [`Error::to_control_flow`]).
    fn is_interrupt(&self) -> bool {
        false
    }

    /// Whether the builtin's name goes before the message, as Bash's `builtin_error`
    /// puts it (`cd: /x: No such file or directory`). A file `.` cannot find is
    /// named alone, as Bash's `file_error` does.
    fn names_its_builtin(&self) -> bool {
        true
    }

    /// Whether this is an error of the assignment a declaration builtin makes, which
    /// abandons the command line as an assignment's error does ([`Error::jumps_to_top_level`]).
    fn is_assignment_error(&self) -> bool {
        false
    }
}

impl BuiltinError for Error {
    fn is_assignment_error(&self) -> bool {
        matches!(self.kind, ErrorKind::AssignmentError(_))
    }

    fn names_its_builtin(&self) -> bool {
        !matches!(
            self.kind,
            ErrorKind::FailedSourcingFile(..) | ErrorKind::AssignmentError(_)
        )
    }

    fn as_io_error(&self) -> Option<&std::io::Error> {
        self.as_io_error()
    }

    fn is_arithmetic_error(&self) -> bool {
        match &self.kind {
            ErrorKind::EvalError(_) => true,
            ErrorKind::BuiltinError(inner, _) => inner.is_arithmetic_error(),
            _ => false,
        }
    }

    fn is_interrupt(&self) -> bool {
        match &self.kind {
            ErrorKind::Interrupted => true,
            ErrorKind::BuiltinError(inner, _) => inner.is_interrupt(),
            _ => false,
        }
    }
}

/// Helper trait for converting values to exit codes.
pub trait ConvertibleToExitCode {
    /// Converts to an exit code.
    fn as_exit_code(&self) -> results::ExecutionExitCode;
}

impl<T> ConvertibleToExitCode for T
where
    results::ExecutionExitCode: for<'a> From<&'a T>,
{
    fn as_exit_code(&self) -> results::ExecutionExitCode {
        self.into()
    }
}

impl From<&ErrorKind> for results::ExecutionExitCode {
    fn from(value: &ErrorKind) -> Self {
        match value {
            ErrorKind::CommandNotFound(..) => Self::NotFound,
            ErrorKind::Unimplemented(..) | ErrorKind::UnimplementedAndTracked(..) => {
                Self::Unimplemented
            }
            ErrorKind::ParseError(..) => Self::InvalidUsage,
            ErrorKind::FunctionParseError(..) => Self::InvalidUsage,
            ErrorKind::TestCommandParseError(..) => Self::InvalidUsage,
            ErrorKind::FailedToExecuteCommand(..) | ErrorKind::CannotExecuteBinaryFile(..) => {
                Self::CannotExecute
            }
            ErrorKind::FunctionNameShadowsSpecialBuiltin { .. } => Self::InvalidUsage,
            ErrorKind::EnvShebang(..) => Self::Custom(125),
            ErrorKind::TestError(_) => Self::InvalidUsage,
            // 128 + SIGINT, the status Bash leaves after Ctrl-C.
            ErrorKind::Interrupted => Self::Interrupted,
            ErrorKind::IoError(io_err) => io_err.into(),
            ErrorKind::BuiltinError(inner, ..) => inner.as_exit_code(),
            _ => Self::GeneralError,
        }
    }
}

impl From<&std::io::Error> for results::ExecutionExitCode {
    fn from(io_err: &std::io::Error) -> Self {
        if io_err.kind() == std::io::ErrorKind::BrokenPipe {
            Self::BrokenPipe
        } else {
            Self::GeneralError
        }
    }
}

impl From<&Error> for results::ExecutionExitCode {
    fn from(error: &Error) -> Self {
        Self::from(&error.kind)
    }
}

impl<T> From<T> for Error
where
    ErrorKind: From<T>,
{
    fn from(convertible_to_kind: T) -> Self {
        Self {
            kind: convertible_to_kind.into(),
            fatal: false,
            location: None,
        }
    }
}

impl Error {
    /// Marks this error as fatal.
    #[must_use]
    pub const fn into_fatal(mut self) -> Self {
        self.fatal = true;
        self
    }

    /// Returns whether or not this error is fatal.
    pub const fn is_fatal(&self) -> bool {
        self.fatal
    }

    /// Whether this error abandons the rest of the command line rather than only failing
    /// its command: an arithmetic error in a builtin other than `let` (`unset 'a[1+]'`,
    /// `printf -v 'a[$i]'` with `array_expand_once`), as Bash's `evalerror` jumps back to
    /// the top level. An arithmetic error in an expansion does so already, by propagating.
    pub fn discards_line(&self) -> bool {
        matches!(&self.kind, ErrorKind::BuiltinError(inner, name)
            if (name != "let" && inner.is_arithmetic_error()) || inner.is_interrupt())
    }

    /// Whether this error abandons the whole top-level command it happened in, however deep
    /// in functions, as Bash's `jump_to_top_level` does, and the shell goes on at the next
    /// one: an assignment to a read-only variable or through a circular name reference,
    /// an arithmetic error in an expansion, and a call past the function nesting limit. A
    /// command that a function's error came out of went on with status 1 (`f; echo same`
    /// echoed); the nesting limit's error left 0. A builtin's own failure, as `read` into
    /// a read-only variable, is not one; the assignment a declaration builtin makes is.
    pub fn jumps_to_top_level(&self) -> bool {
        match &self.kind {
            ErrorKind::ReadonlyVariable(_)
            | ErrorKind::AssignmentError(_)
            | ErrorKind::BadSubstitutionText(_)
            | ErrorKind::CircularNameReference(_)
            | ErrorKind::EvalError(_)
            | ErrorKind::MaxFunctionCallDepthExceeded(..) => true,
            ErrorKind::BuiltinError(inner, _) => inner.is_assignment_error(),
            _ => false,
        }
    }

    /// Whether this is an interrupt that abandons the line with nothing to report: the
    /// line is simply gone, as after Ctrl-C.
    pub fn is_silent_interrupt(&self) -> bool {
        BuiltinError::is_interrupt(self)
    }

    /// Returns a reference to the error kind.
    pub const fn kind(&self) -> &ErrorKind {
        &self.kind
    }

    /// Try to extract a reference to the underlying `std::io::Error`, if any.
    pub fn as_io_error(&self) -> Option<&std::io::Error> {
        match &self.kind {
            ErrorKind::IoError(io_err) => Some(io_err),
            ErrorKind::BuiltinError(inner, _) => inner.as_io_error(),
            _ => None,
        }
    }

    /// Converts this error into the appropriate control flow based on the shell's current state.
    /// This centralizes the logic for determining how fatal errors should affect execution flow.
    ///
    /// An interrupt ends a shell that is not interactive, as SIGINT ends a Bash script;
    /// the interactive shell goes back to its prompt.
    ///
    /// # Arguments
    ///
    /// * `shell` - The shell instance, used to check interactive mode and script call stack.
    pub fn to_control_flow(
        &self,
        shell: &Shell<impl extensions::ShellExtensions>,
    ) -> results::ExecutionControlFlow {
        if (self.is_fatal() || self.is_silent_interrupt()) && !shell.options().interactive {
            results::ExecutionControlFlow::ExitShell
        } else {
            results::ExecutionControlFlow::Normal
        }
    }

    /// Converts this error into an execution result for the shell.
    ///
    /// # Arguments
    ///
    /// * `shell` - The shell instance, used to determine control flow.
    pub fn into_result(
        self,
        shell: &Shell<impl extensions::ShellExtensions>,
    ) -> results::ExecutionResult {
        let next_control_flow = self.to_control_flow(shell);
        // `${u:?}` and an unset variable under `set -u` end a shell that is not
        // interactive with 127, where Bash's top level catches them, a forked pipeline
        // stage's included; a subshell, a compound stage and a command substitution catch
        // them themselves and end with 1 (`execute_in_subshell`). Cash ended with 1.
        let discarded = matches!(
            self.kind,
            ErrorKind::CheckedExpansionError(..) | ErrorKind::ExpandingUnsetVariable(..)
        ) && self.is_fatal()
            && !shell.catches_fatal_errors();
        let exit_code = if (discarded
            || matches!(self.kind, ErrorKind::InvalidParameterTransformation(..)))
            && !shell.options().interactive
        {
            // This is the status used by Bash 5.2 when an invalid `${v@...}`
            // transformation aborts a non-interactive shell. Interactive Bash
            // recovers at the prompt with the ordinary failure status instead.
            results::ExecutionExitCode::NotFound
        } else {
            results::ExecutionExitCode::from(&self)
        };

        results::ExecutionResult {
            next_control_flow,
            exit_code,
        }
    }
}

/// An I/O error worded as the C library's `strerror` words it, which is what Bash and the
/// GNU tools print: `Directory not empty`, not Windows's "The directory is not empty.
/// (os error 145)".
pub fn os_error_text(error: &std::io::Error) -> String {
    use std::io::ErrorKind;
    let text = match error.kind() {
        ErrorKind::NotFound => "No such file or directory",
        ErrorKind::PermissionDenied => "Permission denied",
        ErrorKind::DirectoryNotEmpty => "Directory not empty",
        ErrorKind::AlreadyExists => "File exists",
        ErrorKind::IsADirectory => "Is a directory",
        ErrorKind::NotADirectory => "Not a directory",
        ErrorKind::ResourceBusy => "Device or resource busy",
        _ => {
            let text = error.to_string();
            return match text.find(" (os error ") {
                Some(at) => text.get(..at).unwrap_or_default().to_owned(),
                None => text,
            };
        }
    };
    text.to_owned()
}

impl Error {
    /// This error, happened where `prefix` says (`Shell::error_prefix`), unless it already
    /// says where. An error that abandons the command line is shown once it has left the
    /// function or file it happened in, and Bash names where it happened: the line in
    /// `lib.sh`, not the call in `main.sh`.
    #[must_use]
    pub fn located_at(mut self, prefix: String) -> Self {
        self.location.get_or_insert(prefix);
        self
    }

    /// Where this error happened, as the start of its message, if it left the function or
    /// file it happened in.
    pub fn location(&self) -> Option<&str> {
        self.location.as_deref()
    }

    /// This error's message, with an I/O error worded as the C library words it
    /// ([`os_error_text`]) rather than as Rust does (`i/o error: … (os error 2)`).
    pub fn worded(&self) -> String {
        self.as_io_error()
            .map_or_else(|| self.to_string(), os_error_text)
    }

    /// `error` as one about the file or folder `name`, where it is an I/O error:
    /// `/no/such: No such file or directory`.
    #[must_use]
    pub fn file_error(name: &str, error: Self) -> Self {
        match error.as_io_error() {
            Some(io) => ErrorKind::FileError(
                name.to_owned(),
                std::io::Error::new(io.kind(), io.to_string()),
            )
            .into(),
            None => error,
        }
    }

    /// This error with the variable `name` it is about, where it is one that names a
    /// variable and does not yet: `r: readonly variable`.
    #[must_use]
    pub fn of_variable(mut self, name: &str) -> Self {
        match &mut self.kind {
            ErrorKind::ReadonlyVariable(named)
            | ErrorKind::ConvertingAssociativeArrayToIndexedArray(named)
            | ErrorKind::ConvertingIndexedArrayToAssociativeArray(named)
                if named.is_empty() =>
            {
                name.clone_into(named);
            }
            // `[-3]` becomes `a[-3]`, as Bash names the element of an assignment.
            ErrorKind::ArrayIndexOutOfRange(element) if element.starts_with('[') => {
                element.insert_str(0, name);
            }
            _ => {}
        }
        self
    }
}

/// `name: ` before a message about it, or nothing when it has no name.
fn named(name: &str) -> String {
    if name.is_empty() {
        String::new()
    } else {
        format!("{name}: ")
    }
}

/// Bash's words for a command it cannot find: `x: command not found`, or for a path,
/// `./x: No such file or directory`.
fn command_not_found(name: &str) -> String {
    if name.contains(['/', '\\']) {
        format!("{name}: No such file or directory")
    } else {
        format!("{name}: command not found")
    }
}

/// Convenience function for returning an error for unimplemented functionality.
///
/// # Arguments
///
/// * `msg` - The message to include in the error
pub fn unimp<T>(msg: &'static str) -> Result<T, Error> {
    Err(ErrorKind::Unimplemented(msg).into())
}

/// Convenience function for returning an error for *tracked*, unimplemented functionality.
///
/// # Arguments
///
/// * `msg` - The message to include in the error
/// * `project_issue_id` - The GitHub issue ID where the implementation is tracked.
pub fn unimp_with_issue<T>(msg: &'static str, project_issue_id: u32) -> Result<T, Error> {
    Err(ErrorKind::UnimplementedAndTracked(msg, project_issue_id).into())
}
