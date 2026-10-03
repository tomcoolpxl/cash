//! `$RANDOM`, as Bash 5.3 makes it.
//!
//! Bash's generator, so a script that seeds it (`RANDOM=42`) gets Bash's numbers: the
//! Park-Miller "minimal standard" generator, its 32 bits folded to 15, and never the same
//! number twice in a row (`variables.c`: `brand`, `get_random_number`). An assignment
//! seeds it; it was dropped, so `RANDOM=42` did not seed (LANG-23). A subshell starts a
//! new sequence, as each Bash subshell reseeds.

use std::sync::atomic::{AtomicU64, Ordering};

/// The generator's state: the seed in the low 32 bits, the last number above them. It is
/// read through `&self`, as a dynamic variable's getter only has that, and one shell is
/// only used by one thread at a time.
#[derive(Debug)]
pub(crate) struct ShellRandom(AtomicU64);

impl Default for ShellRandom {
    /// A generator seeded from the operating system's randomness.
    fn default() -> Self {
        let generator = Self(AtomicU64::new(0));
        generator.seed(u64::from(rand::random::<u32>()));
        generator
    }
}

impl ShellRandom {
    /// Seeds it, as `RANDOM=seed` does; Bash keeps the low 32 bits of the number.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "Bash keeps the low 32 bits"
    )]
    pub(crate) fn seed(&self, seed: u64) {
        self.0.store(u64::from(seed as u32), Ordering::Relaxed);
    }

    /// The next number, from 0 to 32767.
    pub(crate) fn next(&self) -> u16 {
        let state = self.0.load(Ordering::Relaxed);
        #[expect(clippy::cast_possible_truncation, reason = "the halves of the state")]
        let (mut seed, last) = (state as u32, (state >> 32) as u16);
        let value = loop {
            seed = park_miller(seed);
            let value = (((seed >> 16) ^ (seed & 0xffff)) & 0x7fff) as u16;
            if value != last {
                break value;
            }
        };
        self.0.store(
            (u64::from(value) << 32) | u64::from(seed),
            Ordering::Relaxed,
        );
        value
    }
}

/// One step of the minimal standard generator, by Schrage's method, as Bash's `intrand32`.
fn park_miller(last: u32) -> u32 {
    let last = if last == 0 {
        123_459_876
    } else {
        i64::from(last)
    };
    let high = last / 127_773;
    let low = last - 127_773 * high;
    let mut next = 16_807 * low - 2_836 * high;
    if next < 0 {
        next += 0x7fff_ffff;
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "in 0..2^31"
    )]
    let next = next as u32;
    next
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first_numbers(seed: u64, count: usize) -> Vec<u16> {
        let generator = ShellRandom::default();
        generator.seed(seed);
        (0..count).map(|_| generator.next()).collect()
    }

    #[test]
    fn a_seeded_sequence_is_bashs() {
        // `RANDOM=42; echo $RANDOM $RANDOM $RANDOM` and the rest, in Git Bash 5.3.
        assert_eq!(first_numbers(42, 3), [17772, 26794, 1435]);
        assert_eq!(first_numbers(0, 2), [20814, 24386]);
        assert_eq!(first_numbers(u64::MAX, 1), [16807]);
        assert_eq!(first_numbers(4_294_967_338, 1), [17772]);
    }
}
