//! Compression on every core: a stream cut into chunks, each compressed on a thread of
//! its own, the results written in the order of the chunks, so the output is the same
//! for any number of threads.

use std::io;
use std::num::NonZeroUsize;

/// The threads a `-T N` asks for: N, or one for each processor core when N is 0.
#[must_use]
pub fn threads(requested: u32) -> usize {
    match usize::try_from(requested) {
        Ok(0) | Err(_) => std::thread::available_parallelism().map_or(1, NonZeroUsize::get),
        Ok(n) => n,
    }
}

/// `chunks` each made into a result by `work` on a thread of its own, all at once, the
/// results in the chunks' order. One chunk is worked on the calling thread.
///
/// # Errors
///
/// The first chunk's error, in their order; a thread that panicked is an error too.
pub fn map_ordered<T, R>(
    chunks: Vec<T>,
    work: impl Fn(T) -> io::Result<R> + Sync,
) -> io::Result<Vec<R>>
where
    T: Send,
    R: Send,
{
    if chunks.len() <= 1 {
        return chunks.into_iter().map(&work).collect();
    }
    let work = &work;
    std::thread::scope(|scope| {
        let handles: Vec<_> = chunks
            .into_iter()
            .map(|chunk| scope.spawn(move || work(chunk)))
            .collect();
        handles
            .into_iter()
            .map(|handle| {
                handle
                    .join()
                    .unwrap_or_else(|_| Err(io::Error::other("a compressing thread failed")))
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn results_keep_the_order_of_their_chunks() {
        let out = map_ordered((0..32u32).collect(), |n| Ok(n * 2)).unwrap();
        assert_eq!(out, (0..32u32).map(|n| n * 2).collect::<Vec<_>>());
        let failed = map_ordered(vec![1, 2, 3], |n| {
            if n == 2 {
                Err(io::Error::other("two"))
            } else {
                Ok(n)
            }
        });
        assert_eq!(failed.unwrap_err().to_string(), "two");
    }

    #[test]
    fn zero_threads_is_one_per_core() {
        assert!(threads(0) >= 1);
        assert_eq!(threads(3), 3);
    }
}
