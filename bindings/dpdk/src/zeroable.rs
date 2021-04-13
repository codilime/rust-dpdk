use std::mem::MaybeUninit;

/// Traits for `zeroable` structures.
///
/// Related issue: https://github.com/rust-lang/rfcs/issues/2626
///
/// DPDK provides customizable per-packet metadata. However, it is initialized via
/// `memset(.., 0, ..)`, and its destructor is not called.
/// A structure must be safe from `MaybeUninit::zeroed().assume_init()`
/// and it must not implement `Drop` trait.
pub unsafe trait Zeroable: Sized {
    fn zeroed() -> Self {
        // Safety: contraints from this trait.
        unsafe { MaybeUninit::zeroed().assume_init() }
    }
}

unsafe impl Zeroable for () {}
