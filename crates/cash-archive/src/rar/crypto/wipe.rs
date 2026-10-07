//! Secrets cleared once they are used, where rars cleared them with the `zeroize` crate:
//! overwritten with zeros, `black_box` keeping the writes from being optimized away.

use std::ops::{Deref, DerefMut};

/// What can be cleared.
pub(crate) trait Wipe {
    /// Overwrites the value with zeros.
    fn wipe(&mut self);
}

impl<T: Copy + Default, const N: usize> Wipe for [T; N] {
    fn wipe(&mut self) {
        self.fill(T::default());
        std::hint::black_box(&*self);
    }
}

impl<T: Copy + Default> Wipe for Vec<T> {
    fn wipe(&mut self) {
        self.fill(T::default());
        std::hint::black_box(&*self);
    }
}

/// A value cleared when it is dropped (`zeroize::Zeroizing`).
pub(crate) struct Wiped<T: Wipe>(T);

impl<T: Wipe> Wiped<T> {
    /// `value`, to be cleared when dropped.
    pub(crate) const fn new(value: T) -> Self {
        Self(value)
    }
}

impl<T: Wipe> Deref for Wiped<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T: Wipe> DerefMut for Wiped<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T: Wipe> Drop for Wiped<T> {
    fn drop(&mut self) {
        self.0.wipe();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wiped_value_is_zeros() {
        let mut key = [7u8; 16];
        key.wipe();
        assert_eq!(key, [0; 16]);
        let mut held = Wiped::new([1u32; 4]);
        held[0] = 9;
        assert_eq!(held[0], 9);
    }
}
