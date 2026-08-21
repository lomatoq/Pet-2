use std::{
    array,
    cell::UnsafeCell,
    mem::MaybeUninit,
    sync::atomic::{AtomicUsize, Ordering},
};

/// Fixed-capacity single-producer/single-consumer ring.
///
/// `push` belongs to the main thread and `pop` belongs to the audio callback. Both
/// operations are bounded, lock-free, allocation-free, and non-blocking.
pub struct SpscRing<T: Copy + Send, const N: usize> {
    slots: [UnsafeCell<MaybeUninit<T>>; N],
    head: AtomicUsize,
    tail: AtomicUsize,
}

unsafe impl<T: Copy + Send, const N: usize> Send for SpscRing<T, N> {}
unsafe impl<T: Copy + Send, const N: usize> Sync for SpscRing<T, N> {}

impl<T: Copy + Send, const N: usize> SpscRing<T, N> {
    #[must_use]
    pub fn new() -> Self {
        assert!(N >= 2, "SPSC ring needs at least two slots");
        Self {
            slots: array::from_fn(|_| UnsafeCell::new(MaybeUninit::uninit())),
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }

    pub fn push(&self, value: T) -> Result<(), T> {
        let head = self.head.load(Ordering::Relaxed);
        let next = (head + 1) % N;
        if next == self.tail.load(Ordering::Acquire) {
            return Err(value);
        }
        unsafe { (*self.slots[head].get()).write(value) };
        self.head.store(next, Ordering::Release);
        Ok(())
    }

    pub fn pop(&self) -> Option<T> {
        let tail = self.tail.load(Ordering::Relaxed);
        if tail == self.head.load(Ordering::Acquire) {
            return None;
        }
        let value = unsafe { (*self.slots[tail].get()).assume_init_read() };
        self.tail.store((tail + 1) % N, Ordering::Release);
        Some(value)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tail.load(Ordering::Acquire) == self.head.load(Ordering::Acquire)
    }
}

impl<T: Copy + Send, const N: usize> Default for SpscRing<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_is_fifo_and_bounded() {
        let ring = SpscRing::<u32, 4>::new();
        assert!(ring.push(1).is_ok());
        assert!(ring.push(2).is_ok());
        assert!(ring.push(3).is_ok());
        assert_eq!(ring.push(4), Err(4));
        assert_eq!(ring.pop(), Some(1));
        assert_eq!(ring.pop(), Some(2));
        assert!(ring.push(4).is_ok());
        assert_eq!(ring.pop(), Some(3));
        assert_eq!(ring.pop(), Some(4));
        assert_eq!(ring.pop(), None);
    }
}
