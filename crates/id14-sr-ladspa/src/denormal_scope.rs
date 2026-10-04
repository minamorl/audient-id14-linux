//! Bound the cost of decaying filter tails for one audio callback.
//!
//! No signal is added, no filter state is reset, and no frames are skipped.
//! On x86_64, DAZ/FTZ are enabled and the caller's floating-point environment
//! is restored when the callback ends. On every other target, including macOS
//! aarch64, this guard is a no-op: it does not mitigate denormal processing cost.
use std::marker::PhantomData;

#[cfg(target_arch = "x86_64")]
const DENORMALS_ARE_ZERO: u32 = 1 << 6;
#[cfg(target_arch = "x86_64")]
const FLUSH_TO_ZERO: u32 = 1 << 15;

#[cfg(target_arch = "x86_64")]
#[inline]
fn read_control() -> u32 {
    let mut control = 0u32;
    // SAFETY: x86_64 has SSE2; the operand points to writable four-byte storage.
    unsafe {
        std::arch::asm!(
            "stmxcsr [{address}]",
            address = in(reg) &mut control,
            options(nostack, preserves_flags),
        );
    }
    control
}

#[cfg(target_arch = "x86_64")]
#[inline]
fn write_control(control: u32) {
    // SAFETY: control comes from MXCSR with only the DAZ/FTZ mode bits changed.
    // The operand remains valid for this instruction; reserved bits are preserved.
    unsafe {
        std::arch::asm!(
            "ldmxcsr [{address}]",
            address = in(reg) &control,
            options(nostack, preserves_flags),
        );
    }
}

/// This guard must be dropped on the thread on which it was acquired.
#[must_use]
pub(crate) struct Scope {
    #[cfg(target_arch = "x86_64")]
    saved: u32,
    _same_thread: PhantomData<*mut ()>,
}

impl Scope {
    #[inline]
    pub(crate) fn enter() -> Self {
        #[cfg(target_arch = "x86_64")]
        let saved = read_control();
        #[cfg(target_arch = "x86_64")]
        write_control(saved | DENORMALS_ARE_ZERO | FLUSH_TO_ZERO);
        Self {
            #[cfg(target_arch = "x86_64")]
            saved,
            _same_thread: PhantomData,
        }
    }
}

impl Drop for Scope {
    #[inline]
    fn drop(&mut self) {
        #[cfg(target_arch = "x86_64")]
        write_control(self.saved);
    }
}

#[cfg(all(test, target_arch = "x86_64"))]
mod tests {
    use super::*;

    #[test]
    fn restores_callers_environment_after_nested_scopes() {
        let initial = read_control();
        {
            let _outer = Scope::enter();
            let active = read_control();
            assert_eq!(active, initial | DENORMALS_ARE_ZERO | FLUSH_TO_ZERO);
            { let _inner = Scope::enter(); }
            assert_eq!(read_control(), active);
        }
        assert_eq!(read_control(), initial);
    }

    #[test]
    fn restores_callers_environment_on_unwind() {
        let initial = read_control();
        let result = std::panic::catch_unwind(|| {
            let _scope = Scope::enter();
            panic!("exercise unwind");
        });
        assert!(result.is_err());
        assert_eq!(read_control(), initial);
    }
}
