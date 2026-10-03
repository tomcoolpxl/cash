//! Exit code mapping — **D15**.
//!
//! Windows exit codes are a full `DWORD`; bash's `$?` is 0–255. Truncating to the low
//! byte matches bash for ordinary codes, but is actively unsafe on its own: a process
//! that dies of `STATUS_ACCESS_VIOLATION` (`0xC0000005`) would report `5`, and one that
//! dies of `0xC0000100` would report **`0` — a crash indistinguishable from success**.
//!
//! So NTSTATUS exception codes are mapped the way bash reports fatal signals, `128 + n`.
//! An access violation becomes `139`, exactly what a segfault yields on Linux, and a
//! script testing for `139` works unchanged.

/// Bash reports a process killed by signal `n` as `128 + n`.
const SIGNAL_BASE: u32 = 128;

/// The exit code Windows gives a process that Ctrl-C ended: `STATUS_CONTROL_C_EXIT`.
///
/// It is what a program without a handler of its own exits with, and what `ping.exe`,
/// Python and Go programs exit with when they let the interrupt end them. Bash reports a
/// command SIGINT ended as 130, so that is what `$?` says here; it said 137, the status of
/// a kill, until the shell came to act on an interrupted command (2026-09-30).
#[expect(
    clippy::unreadable_literal,
    reason = "NTSTATUS values are quoted everywhere as eight unbroken hex digits"
)]
pub const CONTROL_C_EXIT: u32 = 0xC000013A;

// Signal numbers as bash reports them, for the crashes Windows actually produces.
const SIGINT: u32 = 2;
const SIGILL: u32 = 4;
const SIGABRT: u32 = 6;
const SIGFPE: u32 = 8;
const SIGKILL: u32 = 9;
const SIGSEGV: u32 = 11;
const SIGPIPE: u32 = 13;

/// NTSTATUS values that mean "this process crashed", mapped to the signal bash would
/// have reported on Linux.
///
/// Deliberately a small hand-maintained table rather than a range heuristic: D15 accepts
/// that cost, because the whole point is producing the *specific* values scripts compare
/// against.
#[expect(
    clippy::unreadable_literal,
    reason = "NTSTATUS values are quoted everywhere as eight unbroken hex digits; \n              `0xC000_0005` is harder to match against Microsoft's documentation"
)]
const NTSTATUS_SIGNALS: &[(u32, u32)] = &[
    (0xC0000005, SIGSEGV),    // STATUS_ACCESS_VIOLATION          -> 139
    (0xC00000FD, SIGSEGV),    // STATUS_STACK_OVERFLOW            -> 139
    (0xC0000006, SIGSEGV),    // STATUS_IN_PAGE_ERROR             -> 139
    (0xC0000008, SIGSEGV),    // STATUS_INVALID_HANDLE            -> 139
    (0xC000001D, SIGILL),     // STATUS_ILLEGAL_INSTRUCTION       -> 132
    (0xC000001E, SIGILL),     // STATUS_INVALID_LOCK_SEQUENCE     -> 132
    (0xC0000025, SIGILL),     // STATUS_NONCONTINUABLE_EXCEPTION  -> 132
    (0xC0000026, SIGILL),     // STATUS_INVALID_DISPOSITION       -> 132
    (0xC000008C, SIGSEGV),    // STATUS_ARRAY_BOUNDS_EXCEEDED     -> 139
    (0xC000008D, SIGFPE),     // STATUS_FLOAT_DENORMAL_OPERAND    -> 136
    (0xC000008E, SIGFPE),     // STATUS_FLOAT_DIVIDE_BY_ZERO      -> 136
    (0xC000008F, SIGFPE),     // STATUS_FLOAT_INEXACT_RESULT      -> 136
    (0xC0000090, SIGFPE),     // STATUS_FLOAT_INVALID_OPERATION   -> 136
    (0xC0000091, SIGFPE),     // STATUS_FLOAT_OVERFLOW            -> 136
    (0xC0000092, SIGFPE),     // STATUS_FLOAT_STACK_CHECK         -> 136
    (0xC0000093, SIGFPE),     // STATUS_FLOAT_UNDERFLOW           -> 136
    (0xC0000094, SIGFPE),     // STATUS_INTEGER_DIVIDE_BY_ZERO    -> 136
    (0xC0000095, SIGFPE),     // STATUS_INTEGER_OVERFLOW          -> 136
    (0xC0000096, SIGILL),     // STATUS_PRIVILEGED_INSTRUCTION    -> 132
    (0xC0000409, SIGABRT),    // STATUS_STACK_BUFFER_OVERRUN      -> 134
    (0xC0000374, SIGABRT),    // STATUS_HEAP_CORRUPTION           -> 134
    (CONTROL_C_EXIT, SIGINT), // STATUS_CONTROL_C_EXIT         -> 130
    (0xC00000B1, SIGPIPE),    // STATUS_PIPE_BROKEN               -> 141
    (0xC000014B, SIGPIPE),    // STATUS_PIPE_BROKEN (alt)         -> 141
];

/// Map a Windows process exit code to the value `$?` should report (D15).
#[must_use]
pub fn from_windows(code: u32) -> u8 {
    if let Some(&(_, signal)) = NTSTATUS_SIGNALS.iter().find(|&&(status, _)| status == code) {
        return u8::try_from(SIGNAL_BASE + signal).unwrap_or(u8::MAX);
    }

    // An unrecognised NTSTATUS-shaped failure would truncate to something arbitrary, and
    // could truncate to zero. Report a generic "killed by signal" instead of lying.
    if is_ntstatus_error(code) {
        return u8::try_from(SIGNAL_BASE + SIGKILL).unwrap_or(u8::MAX);
    }

    // Ordinary exit codes truncate to the low byte, exactly as bash does.
    // §9 measured this already working: cmd.exe /c "exit 300" gives 44.
    u8::try_from(code & 0xFF).unwrap_or(u8::MAX)
}

/// Whether a code is in the NTSTATUS error range (severity bits set to `11`).
///
/// These are produced by the kernel when a process dies of an exception, not by the
/// process calling `exit()`.
#[must_use]
pub const fn is_ntstatus_error(code: u32) -> bool {
    // Severity occupies the top two bits; 0b11 is STATUS_SEVERITY_ERROR.
    (code >> 30) == 0b11
}

/// Whether the mapped status indicates the process was killed rather than exiting.
#[must_use]
pub const fn was_killed(code: u32) -> bool {
    is_ntstatus_error(code)
}
