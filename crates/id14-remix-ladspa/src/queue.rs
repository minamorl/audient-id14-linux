//! Bounded single-producer/single-consumer transport. No allocation or lock in push/pop.
use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

struct Ring<T, const N: usize> {
    slots: [UnsafeCell<MaybeUninit<T>>; N],
    read: AtomicUsize,
    write: AtomicUsize,
}
// Exactly one Producer and one Consumer are constructed. Release/acquire transfers ownership.
unsafe impl<T: Send, const N: usize> Sync for Ring<T, N> {}
impl<T, const N: usize> Drop for Ring<T, N> {
    fn drop(&mut self) {
        let mut r = *self.read.get_mut();
        let w = *self.write.get_mut();
        while r != w {
            unsafe { self.slots[r % N].get_mut().assume_init_drop() };
            r = r.wrapping_add(1);
        }
    }
}
pub struct Producer<T, const N: usize>(Arc<Ring<T, N>>);
pub struct Consumer<T, const N: usize>(Arc<Ring<T, N>>);

pub fn channel<T: Send, const N: usize>() -> (Producer<T, N>, Consumer<T, N>) {
    assert!(N > 0);
    let ring = Arc::new(Ring {
        slots: std::array::from_fn(|_| UnsafeCell::new(MaybeUninit::uninit())),
        read: AtomicUsize::new(0),
        write: AtomicUsize::new(0),
    });
    (Producer(ring.clone()), Consumer(ring))
}
impl<T, const N: usize> Producer<T, N> {
    pub fn push(&mut self, value: T) -> Result<(), T> {
        let w = self.0.write.load(Ordering::Relaxed);
        if w.wrapping_sub(self.0.read.load(Ordering::Acquire)) == N {
            return Err(value);
        }
        unsafe { (*self.0.slots[w % N].get()).write(value) };
        self.0.write.store(w.wrapping_add(1), Ordering::Release);
        Ok(())
    }
}
impl<T, const N: usize> Consumer<T, N> {
    pub fn pop(&mut self) -> Option<T> {
        let r = self.0.read.load(Ordering::Relaxed);
        if r == self.0.write.load(Ordering::Acquire) {
            return None;
        }
        let value = unsafe { (*self.0.slots[r % N].get()).assume_init_read() };
        self.0.read.store(r.wrapping_add(1), Ordering::Release);
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn concurrent_order_and_backpressure() {
        let (mut tx, mut rx) = channel::<usize, 8>();
        let worker = std::thread::spawn(move || {
            for expected in 0..100_000 {
                loop {
                    if let Some(value) = rx.pop() {
                        assert_eq!(value, expected);
                        break;
                    }
                    std::thread::yield_now();
                }
            }
        });
        for value in 0..100_000 {
            while tx.push(value).is_err() {
                std::thread::yield_now();
            }
        }
        worker.join().unwrap();
    }
}
