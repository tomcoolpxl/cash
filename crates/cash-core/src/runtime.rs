//! Running shell futures where no `.await` can: from synchronous code, and on threads of
//! the shell's own.
//!
//! The shell drives its commands on a multi-thread tokio runtime, whose handle any
//! thread can block on: its workers drive the I/O and timers. An embedder, or a test,
//! may drive it on a current-thread runtime instead, where only `Runtime::block_on`
//! drives them and `tokio::task::block_in_place` panics. Neither may then be used
//! (XC-19).

use std::future::Future;

use tokio::runtime::{Handle, RuntimeFlavor};

/// The current runtime's handle, when a thread of the shell's own may block on it: a
/// multi-thread runtime's, whose workers drive its I/O and timers for every thread.
#[must_use]
pub fn shareable_handle() -> Option<Handle> {
    Handle::try_current()
        .ok()
        .filter(|handle| handle.runtime_flavor() == RuntimeFlavor::MultiThread)
}

/// Runs the future `make` returns to its end on this thread, which is not one of a
/// runtime's: on `handle` when there is one (see [`shareable_handle`]), else on a
/// current-thread runtime of its own.
///
/// # Errors
///
/// When there is no handle and no runtime could be built.
pub fn block_on_this_thread<F: Future>(
    handle: Option<&Handle>,
    make: impl FnOnce() -> F,
) -> std::io::Result<F::Output> {
    match handle {
        Some(handle) => Ok(handle.block_on(make())),
        None => Ok(tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(make())),
    }
}

/// Runs the future `make` returns to its end from synchronous code running on a
/// runtime, as `read -e`'s Tab does to complete a word.
///
/// On a multi-thread runtime the worker steps aside for it (`block_in_place`); on a
/// current-thread runtime, which cannot, the future runs on a thread of its own, with a
/// runtime of its own, while this one waits.
///
/// # Errors
///
/// When the thread or its runtime could not be made.
pub fn block_on_from_sync<T: Send, F: Future<Output = T>>(
    make: impl FnOnce() -> F + Send,
) -> std::io::Result<T> {
    if let Some(handle) = shareable_handle() {
        return Ok(tokio::task::block_in_place(|| handle.block_on(make())));
    }
    std::thread::scope(|scope| {
        let thread = std::thread::Builder::new()
            .name("cash-block-on".into())
            .stack_size(crate::SHELL_THREAD_STACK_SIZE)
            .spawn_scoped(scope, || block_on_this_thread(None, make))?;
        thread
            .join()
            .unwrap_or_else(|payload| std::panic::resume_unwind(payload))
    })
}

#[cfg(test)]
mod tests {
    use super::{block_on_from_sync, shareable_handle};

    /// A future that needs the runtime's timer, which only a driven runtime fires.
    async fn needs_the_timer() -> u32 {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        7
    }

    fn from_sync() -> u32 {
        block_on_from_sync(needs_the_timer).unwrap()
    }

    #[test]
    fn sync_code_blocks_on_a_future_under_either_runtime() {
        let current = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        assert!(current.block_on(async { shareable_handle() }).is_none());
        assert_eq!(current.block_on(async { from_sync() }), 7);

        let multi = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        assert!(multi.block_on(async { shareable_handle() }).is_some());
        assert_eq!(
            multi.block_on(async { tokio::spawn(async { from_sync() }).await.unwrap() }),
            7
        );
    }
}
