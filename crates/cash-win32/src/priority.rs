//! A process's priority class, for `nice` and `renice`.
//!
//! Windows schedules by six priority classes where Unix has forty nice values. The two
//! are mapped here, once, in both directions: a niceness picks the class whose range it
//! falls in, and a class reads back as the middle of its range, so that `nice -n 10 nice`
//! prints what a later `renice` will see.
//!
//! `nice COMMAND` creates the program in the class, through a `CreateProcess` creation
//! flag ([`PriorityClass::raw`]): a child inherits its parent's class only when that is
//! idle or below normal, and is created normal under a parent of any higher class, so
//! taking the class for the shell itself ([`hold`]) would raise nothing.

use std::io;
use std::os::windows::io::AsRawHandle as _;

use windows_sys::Win32::System::Threading::{
    ABOVE_NORMAL_PRIORITY_CLASS, BELOW_NORMAL_PRIORITY_CLASS, GetCurrentProcess, GetCurrentThread,
    GetPriorityClass, GetThreadPriority, HIGH_PRIORITY_CLASS, IDLE_PRIORITY_CLASS,
    NORMAL_PRIORITY_CLASS, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_INFORMATION,
    REALTIME_PRIORITY_CLASS, SetPriorityClass, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL,
};

use crate::handle::open_process;

/// The lowest niceness, Unix's most favourable.
pub const MIN_NICENESS: i32 = -20;
/// The highest niceness, Unix's least favourable.
pub const MAX_NICENESS: i32 = 19;

/// One of Windows' six priority classes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PriorityClass {
    /// Runs only when nothing else wants a processor.
    Idle,
    /// Below the default.
    BelowNormal,
    /// The default class of a program started from the desktop or a console.
    Normal,
    /// Above the default.
    AboveNormal,
    /// Ahead of everything a user normally runs; the Task Manager's "High".
    High,
    /// Ahead of the system itself; needs a privilege, and `nice` never asks for it.
    Realtime,
}

impl PriorityClass {
    /// The class for a niceness, clamped to -20..19: -20..-11 is high, -10..-1 above
    /// normal, 0 normal, 1..10 below normal, 11..19 idle. The default `nice` adjustment,
    /// 10, lands in below normal: a `nice make` still gets its share of the processor.
    #[must_use]
    pub const fn for_niceness(niceness: i32) -> Self {
        match niceness {
            i32::MIN..=-11 => Self::High,
            -10..=-1 => Self::AboveNormal,
            0 => Self::Normal,
            1..=10 => Self::BelowNormal,
            _ => Self::Idle,
        }
    }

    /// The niceness this class reads back as: the middle of its range (realtime, which
    /// `nice` never sets, reads as -20).
    #[must_use]
    pub const fn niceness(self) -> i32 {
        match self {
            Self::Realtime => MIN_NICENESS,
            Self::High => -15,
            Self::AboveNormal => -5,
            Self::Normal => 0,
            Self::BelowNormal => 5,
            Self::Idle => 15,
        }
    }

    /// The class `GetPriorityClass` reported, or `None` for a value it does not return.
    #[must_use]
    pub const fn from_raw(raw: u32) -> Option<Self> {
        match raw {
            IDLE_PRIORITY_CLASS => Some(Self::Idle),
            BELOW_NORMAL_PRIORITY_CLASS => Some(Self::BelowNormal),
            NORMAL_PRIORITY_CLASS => Some(Self::Normal),
            ABOVE_NORMAL_PRIORITY_CLASS => Some(Self::AboveNormal),
            HIGH_PRIORITY_CLASS => Some(Self::High),
            REALTIME_PRIORITY_CLASS => Some(Self::Realtime),
            _ => None,
        }
    }

    /// The value `SetPriorityClass` takes, also a `CreateProcess` creation flag.
    #[must_use]
    pub const fn raw(self) -> u32 {
        match self {
            Self::Idle => IDLE_PRIORITY_CLASS,
            Self::BelowNormal => BELOW_NORMAL_PRIORITY_CLASS,
            Self::Normal => NORMAL_PRIORITY_CLASS,
            Self::AboveNormal => ABOVE_NORMAL_PRIORITY_CLASS,
            Self::High => HIGH_PRIORITY_CLASS,
            Self::Realtime => REALTIME_PRIORITY_CLASS,
        }
    }

    /// The class's name as Task Manager and PowerShell's `Get-Process` spell it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::BelowNormal => "BelowNormal",
            Self::Normal => "Normal",
            Self::AboveNormal => "AboveNormal",
            Self::High => "High",
            Self::Realtime => "RealTime",
        }
    }
}

/// Clamps a niceness to -20..19, as `setpriority` does silently.
#[must_use]
pub const fn clamp_niceness(niceness: i64) -> i32 {
    if niceness < MIN_NICENESS as i64 {
        MIN_NICENESS
    } else if niceness > MAX_NICENESS as i64 {
        MAX_NICENESS
    } else {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "in -20..19, checked just above"
        )]
        let in_range = niceness as i32;
        in_range
    }
}

/// The class of the process behind `handle`.
fn class_of(handle: windows_sys::Win32::Foundation::HANDLE) -> io::Result<PriorityClass> {
    // SAFETY: the handle is open and carries PROCESS_QUERY_LIMITED_INFORMATION.
    let raw = unsafe { GetPriorityClass(handle) };
    if raw == 0 {
        return Err(io::Error::last_os_error());
    }
    PriorityClass::from_raw(raw)
        .ok_or_else(|| io::Error::other(std::format!("unknown priority class {raw:#x}")))
}

/// Sets the class of the process behind `handle`.
fn set_class_of(
    handle: windows_sys::Win32::Foundation::HANDLE,
    class: PriorityClass,
) -> io::Result<()> {
    // SAFETY: the handle is open and carries PROCESS_SET_INFORMATION.
    if unsafe { SetPriorityClass(handle, class.raw()) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// This process's own class.
///
/// # Errors
///
/// When Windows will not say, which it does for every process of its own.
pub fn current() -> io::Result<PriorityClass> {
    // SAFETY: a pseudo-handle for this process, which needs no closing.
    let me = unsafe { GetCurrentProcess() };
    class_of(me)
}

/// Sets this process's own class. Raising it as far as high needs no privilege, unlike a
/// negative niceness on Unix; realtime does.
///
/// # Errors
///
/// When Windows refuses, as it does realtime without `SeIncreaseBasePriorityPrivilege`.
pub fn set_current(class: PriorityClass) -> io::Result<()> {
    // SAFETY: a pseudo-handle for this process, which needs no closing.
    let me = unsafe { GetCurrentProcess() };
    set_class_of(me, class)
}

/// The class of the process `pid`.
///
/// # Errors
///
/// `ERROR_INVALID_PARAMETER` (87) when there is no such process, `ERROR_ACCESS_DENIED`
/// (5) when it may not be opened even to ask.
pub fn of_process(pid: u32) -> io::Result<PriorityClass> {
    let process = open_process(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
    class_of(process.as_raw_handle())
}

/// Sets the class of the process `pid`.
///
/// # Errors
///
/// `ERROR_INVALID_PARAMETER` (87) when there is no such process, `ERROR_ACCESS_DENIED`
/// (5) when it is another user's, elevated, or protected by Windows.
pub fn set_process(pid: u32, class: PriorityClass) -> io::Result<()> {
    let process = open_process(
        pid,
        PROCESS_SET_INFORMATION | PROCESS_QUERY_LIMITED_INFORMATION,
    )?;
    set_class_of(process.as_raw_handle(), class)
}

/// This process's class, taken for a while: dropping it puts the previous class back.
#[must_use = "the class is put back when this is dropped"]
pub struct Held {
    previous: Option<PriorityClass>,
}

impl Drop for Held {
    fn drop(&mut self) {
        if let Some(previous) = self.previous {
            let _ = set_current(previous);
        }
    }
}

/// Puts this process in `class` until the returned guard is dropped.
///
/// A program started meanwhile inherits the class when it is idle or below normal, and
/// is created normal otherwise; `nice COMMAND` therefore passes the class at creation
/// instead, and this serves the tests that need a known class to start from.
///
/// # Errors
///
/// When the class cannot be read or set; nothing is changed then.
pub fn hold(class: PriorityClass) -> io::Result<Held> {
    let previous = current()?;
    if previous == class {
        return Ok(Held { previous: None });
    }
    set_current(class)?;
    Ok(Held {
        previous: Some(previous),
    })
}

/// What `GetThreadPriority` answers when it fails (`MAXLONG`).
const THREAD_PRIORITY_ERROR_RETURN: i32 = i32::MAX;

/// The thread that took it, run below normal priority for a while: dropping it, on that
/// thread, puts the thread's previous priority back.
#[must_use = "the thread's priority is put back when this is dropped"]
pub struct ThreadLowered {
    previous: Option<i32>,
    /// The priority is the taking thread's, so the guard stays on it.
    _on_this_thread: std::marker::PhantomData<*const ()>,
}

impl Drop for ThreadLowered {
    fn drop(&mut self) {
        if let Some(previous) = self.previous {
            // SAFETY: a pseudo-handle for this thread, which needs no closing.
            let me = unsafe { GetCurrentThread() };
            // SAFETY: as above; a priority this thread had.
            let _ = unsafe { SetThreadPriority(me, previous) };
        }
    }
}

/// Runs this thread one step below normal priority until the returned guard is dropped.
///
/// For the threads of a background job: as threads of the shell's own process they
/// would share the processors with the foreground turn by turn, and Windows Server's
/// long time slices leave a foreground that wakes waiting behind every busy job. One
/// step below, the foreground takes a processor the moment it wakes, and the jobs have
/// every processor nothing else wants. A thread already below normal is left as it is,
/// and so is one whose priority cannot be read or set.
pub fn lower_this_thread() -> ThreadLowered {
    // SAFETY: a pseudo-handle for this thread, which needs no closing.
    let me = unsafe { GetCurrentThread() };
    // SAFETY: as above.
    let previous = unsafe { GetThreadPriority(me) };
    let lowered = previous != THREAD_PRIORITY_ERROR_RETURN
        && previous > THREAD_PRIORITY_BELOW_NORMAL
        // SAFETY: as above.
        && unsafe { SetThreadPriority(me, THREAD_PRIORITY_BELOW_NORMAL) } != 0;
    ThreadLowered {
        previous: lowered.then_some(previous),
        _on_this_thread: std::marker::PhantomData,
    }
}

/// How long a thread runs before [`share_the_processor`] hands its processor on.
const TURN: std::time::Duration = std::time::Duration::from_millis(1);

/// How many calls of [`share_the_processor`] pass between looks at the clock: a loop of
/// builtins calls it every microsecond or two.
const CALLS_BETWEEN_LOOKS: u32 = 8;

/// Hands this thread's processor to another thread ready to run, once this one has run
/// for a millisecond since it last did: for a lowered thread that may never wait.
///
/// A lowered thread that holds a lock the foreground wants (the allocator's, the
/// standard output's, made once) runs again only in its turn among the busy threads of
/// its priority, each turn a whole time slice, 30 to 190 ms; forty busy jobs held the
/// foreground's first `echo` for over a second on one processor. A turn of a millisecond
/// bounds that, at a cost of a system call per millisecond.
pub fn share_the_processor() {
    thread_local! {
        static CALLS_AND_TURN: std::cell::Cell<(u32, Option<std::time::Instant>)> =
            const { std::cell::Cell::new((0, None)) };
    }
    CALLS_AND_TURN.with(|state| {
        let (calls, began) = state.get();
        if calls < CALLS_BETWEEN_LOOKS {
            state.set((calls + 1, began));
            return;
        }
        let now = std::time::Instant::now();
        let began = match began {
            Some(at) if now.duration_since(at) < TURN => at,
            Some(_) => {
                std::thread::yield_now();
                std::time::Instant::now()
            }
            None => now,
        };
        state.set((0, Some(began)));
    });
}

/// This thread's priority, as `SetThreadPriority` takes it: 0 is normal, -1 below.
#[must_use]
pub fn this_threads_priority() -> Option<i32> {
    // SAFETY: a pseudo-handle for this thread, which needs no closing.
    let me = unsafe { GetCurrentThread() };
    // SAFETY: as above.
    let priority = unsafe { GetThreadPriority(me) };
    (priority != THREAD_PRIORITY_ERROR_RETURN).then_some(priority)
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert loudly on failure"
)]
mod tests {
    use super::*;

    #[test]
    fn every_niceness_has_a_class_and_every_class_a_niceness_in_its_own_range() {
        for niceness in MIN_NICENESS..=MAX_NICENESS {
            let class = PriorityClass::for_niceness(niceness);
            assert_eq!(
                PriorityClass::for_niceness(class.niceness()),
                class,
                "{niceness} -> {class:?} -> {}",
                class.niceness()
            );
        }
        assert_eq!(PriorityClass::for_niceness(-20), PriorityClass::High);
        assert_eq!(PriorityClass::for_niceness(-11), PriorityClass::High);
        assert_eq!(PriorityClass::for_niceness(-10), PriorityClass::AboveNormal);
        assert_eq!(PriorityClass::for_niceness(-1), PriorityClass::AboveNormal);
        assert_eq!(PriorityClass::for_niceness(0), PriorityClass::Normal);
        assert_eq!(PriorityClass::for_niceness(1), PriorityClass::BelowNormal);
        assert_eq!(PriorityClass::for_niceness(10), PriorityClass::BelowNormal);
        assert_eq!(PriorityClass::for_niceness(11), PriorityClass::Idle);
        assert_eq!(PriorityClass::for_niceness(19), PriorityClass::Idle);
        // Out of range is clamped by the ranges themselves.
        assert_eq!(PriorityClass::for_niceness(100), PriorityClass::Idle);
        assert_eq!(PriorityClass::for_niceness(-100), PriorityClass::High);
    }

    #[test]
    fn classes_read_back_as_the_middle_of_their_range() {
        assert_eq!(PriorityClass::High.niceness(), -15);
        assert_eq!(PriorityClass::AboveNormal.niceness(), -5);
        assert_eq!(PriorityClass::Normal.niceness(), 0);
        assert_eq!(PriorityClass::BelowNormal.niceness(), 5);
        assert_eq!(PriorityClass::Idle.niceness(), 15);
        assert_eq!(PriorityClass::Realtime.niceness(), -20);
    }

    #[test]
    fn raw_values_round_trip() {
        for class in [
            PriorityClass::Idle,
            PriorityClass::BelowNormal,
            PriorityClass::Normal,
            PriorityClass::AboveNormal,
            PriorityClass::High,
            PriorityClass::Realtime,
        ] {
            assert_eq!(PriorityClass::from_raw(class.raw()), Some(class));
        }
        assert_eq!(PriorityClass::from_raw(0), None);
        assert_eq!(PriorityClass::from_raw(3), None);
    }

    #[test]
    fn clamping() {
        assert_eq!(clamp_niceness(100), 19);
        assert_eq!(clamp_niceness(-100), -20);
        assert_eq!(clamp_niceness(7), 7);
        assert_eq!(clamp_niceness(i64::MAX), 19);
    }

    #[test]
    fn holding_a_class_changes_this_process_and_dropping_puts_it_back() {
        let before = current().expect("own class");
        let other = if before == PriorityClass::BelowNormal {
            PriorityClass::Idle
        } else {
            PriorityClass::BelowNormal
        };
        {
            let _held = hold(other).expect("set own class");
            assert_eq!(current().unwrap(), other);
            assert_eq!(of_process(std::process::id()).unwrap(), other);
        }
        assert_eq!(current().unwrap(), before);
    }

    #[test]
    fn lowering_a_thread_lasts_as_long_as_the_guard() {
        std::thread::spawn(|| {
            assert_eq!(this_threads_priority(), Some(0));
            {
                let _lowered = lower_this_thread();
                assert_eq!(this_threads_priority(), Some(-1));
                // Lowered again, it stays where it is, and the outer guard restores.
                drop(lower_this_thread());
                assert_eq!(this_threads_priority(), Some(-1));
            }
            assert_eq!(this_threads_priority(), Some(0));
        })
        .join()
        .unwrap();
    }

    #[test]
    fn a_missing_process_is_error_87() {
        // Process ids are multiples of four, so this one never exists.
        assert_eq!(of_process(3).unwrap_err().raw_os_error(), Some(87));
        assert_eq!(
            set_process(3, PriorityClass::Normal)
                .unwrap_err()
                .raw_os_error(),
            Some(87)
        );
    }
}
