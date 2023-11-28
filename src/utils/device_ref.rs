use core::ops::Deref;
use core::ptr::NonNull;
use core::marker::PhantomData;

#[derive(Debug, Copy, Clone)]
pub struct DeviceRef<'a, T> {
    ptr: NonNull<T>,
    _maker: PhantomData<&'a T>,
}

impl<T> DeviceRef<'_, T> {
    /// Create a new `DeviceRef` from a raw pointer
    ///
    /// ## Safety
    ///
    /// - `ptr` must be aligned, non-null, and dereferencable as `T`.
    /// - `*ptr` must be valid for the program duration.
    pub const unsafe fn new<'a>(ptr: *const T) -> DeviceRef<'a, T> {
        // SAFETY: `ptr` is non-null as promised by the caller.
        DeviceRef {
            ptr: NonNull::new_unchecked(ptr.cast_mut()),
            _maker: PhantomData,
        }
    }
}

// SAFETY: T provides the necessary guarantees for Sync & Send.
unsafe impl<T: Sync> Send for DeviceRef<'_, T> {}
unsafe impl<T: Sync> Sync for DeviceRef<'_, T> {}

impl<T> Deref for DeviceRef<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: `ptr` is aligned and dereferencable for the program
        // duration as promised by the caller of `DeviceRef::new`.
        unsafe { self.ptr.as_ref() }
    }
}
