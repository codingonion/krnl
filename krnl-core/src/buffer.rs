use crate::scalar::{Scalar, ScalarType};
#[cfg(not(target_arch = "spirv"))]
use core::marker::PhantomData;
use core::ops::Index;
#[cfg(target_arch = "spirv")]
use core::{arch::asm, mem::MaybeUninit};
#[cfg(target_arch = "spirv")]
use spirv_std::arch::IndexUnchecked;

/** Unsafe Index trait.

Like [Index], performs checked indexing, but the caller must ensure that there is no aliasing of a mutable reference.
*/
pub trait UnsafeIndex<Idx> {
    /// The returned type after indexing.
    type Output;
    /// Immutably indexes with `index`.
    /// # Safety
    /// The caller must ensure that the returned reference is not aliased by a mutable borrow, ie by a call to `.unsafe_index_mut()` with the same index.
    unsafe fn unsafe_index(&self, index: Idx) -> &Self::Output;
    /// Mutably indexes with `index`.
    /// # Safety
    /// The caller must ensure that the returned reference is not aliased by another borrow, ie by a call to `.unsafe_index()` or `.unsafe_index_mut()` with the same index.
    #[allow(clippy::mut_from_ref)]
    unsafe fn unsafe_index_mut(&self, index: Idx) -> &mut Self::Output;
}

#[cfg(target_arch = "spirv")]
trait IndexUncheckedMutExt<T> {
    unsafe fn index_unchecked_mut_ext(&self, index: usize) -> &mut T;
}

#[cfg(target_arch = "spirv")]
impl<T, const N: usize> IndexUncheckedMutExt<T> for [T; N] {
    #[inline]
    unsafe fn index_unchecked_mut_ext(&self, index: usize) -> &mut T {
        let mut output = MaybeUninit::uninit();
        unsafe {
            asm!(
                "%val_ptr = OpInBoundsAccessChain _ {array_ptr} {index}",
                "OpStore {output} %val_ptr",
                array_ptr = in(reg) self,
                index = in(reg) index,
                output = in(reg) output.as_mut_ptr(),
            );
            output.assume_init()
        }
    }
}

mod sealed {
    pub trait Sealed {}
}
use sealed::Sealed;

/// Base trait for [`BufferBase`] representation.
#[allow(clippy::len_without_is_empty)]
pub trait DataBase: Sealed {
    /// The numerical type of the buffer.
    type Elem: Scalar;
    #[doc(hidden)]
    fn len(&self) -> usize;
}

/// Marker trait for immutable access.
///
/// See [`Slice`].
pub trait Data: DataBase + Index<usize, Output = Self::Elem> {}
/// Marker trait for unsafe access.
///
/// See [`UnsafeSlice`].
pub trait UnsafeData: DataBase + UnsafeIndex<usize, Output = Self::Elem> {}

/// [`Slice`] representation.
#[derive(Clone, Copy)]
pub struct SliceRepr<'a, T> {
    #[cfg(not(target_arch = "spirv"))]
    inner: &'a [T],
    #[cfg(target_arch = "spirv")]
    inner: &'a [T; 1],
    #[cfg(target_arch = "spirv")]
    offset: usize,
    #[cfg(target_arch = "spirv")]
    len: usize,
}

impl<T> Sealed for SliceRepr<'_, T> {}

impl<T: Scalar> DataBase for SliceRepr<'_, T> {
    type Elem = T;
    #[cfg(not(target_arch = "spirv"))]
    #[inline]
    fn len(&self) -> usize {
        self.inner.len()
    }
    #[cfg(target_arch = "spirv")]
    #[inline]
    fn len(&self) -> usize {
        self.len
    }
}

impl<T: Scalar> Index<usize> for SliceRepr<'_, T> {
    type Output = T;
    #[inline]
    fn index(&self, index: usize) -> &Self::Output {
        #[cfg(target_arch = "spirv")]
        if index < self.len {
            unsafe { self.inner.index_unchecked(self.offset + index) }
        } else {
            let len = self.len;
            panic!("index out of bounds: the len is {index} but the index is {len}")
        }
        #[cfg(not(target_arch = "spirv"))]
        self.inner.index(index)
    }
}

impl<T: Scalar> Data for SliceRepr<'_, T> {}

/// [`UnsafeSlice`] representation.
#[derive(Clone, Copy)]
pub struct UnsafeSliceRepr<'a, T> {
    #[cfg(not(target_arch = "spirv"))]
    ptr: *mut T,
    #[cfg(target_arch = "spirv")]
    #[allow(unused)]
    inner: &'a [T; 1],
    #[cfg(target_arch = "spirv")]
    #[allow(unused)]
    offset: usize,
    len: usize,
    #[cfg(not(target_arch = "spirv"))]
    _m: PhantomData<&'a ()>,
}

impl<T> Sealed for UnsafeSliceRepr<'_, T> {}

impl<T: Scalar> DataBase for UnsafeSliceRepr<'_, T> {
    type Elem = T;
    #[inline]
    fn len(&self) -> usize {
        self.len
    }
}

impl<T: Scalar> UnsafeIndex<usize> for UnsafeSliceRepr<'_, T> {
    type Output = T;
    #[inline]
    unsafe fn unsafe_index(&self, index: usize) -> &Self::Output {
        if index < self.len {
            #[cfg(target_arch = "spirv")]
            unsafe {
                self.inner.index_unchecked(self.offset + index)
            }
            #[cfg(not(target_arch = "spirv"))]
            unsafe {
                &*self.ptr.add(index)
            }
        } else {
            let len = self.len;
            panic!("index out of bounds: the len is {index} but the index is {len}")
        }
    }
    #[inline]
    unsafe fn unsafe_index_mut(&self, index: usize) -> &mut Self::Output {
        if index < self.len {
            #[cfg(target_arch = "spirv")]
            unsafe {
                self.inner.index_unchecked_mut_ext(self.offset + index)
            }
            #[cfg(not(target_arch = "spirv"))]
            unsafe {
                &mut *self.ptr.add(index)
            }
        } else {
            let len = self.len();
            panic!("index out of bounds: the len is {index} but the index is {len}")
        }
    }
}

impl<T: Scalar> UnsafeData for UnsafeSliceRepr<'_, T> {}

unsafe impl<T: Send> Send for UnsafeSliceRepr<'_, T> {}
unsafe impl<T: Sync> Sync for UnsafeSliceRepr<'_, T> {}

/// A buffer.
///
/// [`Slice`] implements [`Index`] and [`UnsafeSlice`] implements [`UnsafeIndex`].
#[derive(Clone, Copy)]
pub struct BufferBase<S> {
    data: S,
}

/// [`Slice`] implements [`Index`].
///
/// See [`BufferBase`].
pub type Slice<'a, T> = BufferBase<SliceRepr<'a, T>>;
/// [`UnsafeSlice`] implements [`UnsafeIndex`].
///
/// See [`BufferBase`].
pub type UnsafeSlice<'a, T> = BufferBase<UnsafeSliceRepr<'a, T>>;

impl<S: DataBase> BufferBase<S> {
    /// The length of the buffer.
    #[inline]
    pub fn len(&self) -> usize {
        self.data.len()
    }
    /// Whether the buffer is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    #[doc(hidden)]
    #[deprecated(since = "0.0.4", note = "use S::Elem::SCALAR_TYPE")]
    #[inline]
    pub fn scalar_type(&self) -> ScalarType {
        S::Elem::SCALAR_TYPE
    }
}

impl<S: Data> Index<usize> for BufferBase<S> {
    type Output = S::Elem;
    #[inline]
    fn index(&self, index: usize) -> &Self::Output {
        self.data.index(index)
    }
}

impl<S: UnsafeData> UnsafeIndex<usize> for BufferBase<S> {
    type Output = S::Elem;
    /// # Safety
    /// The caller must ensure that the returned reference is not aliased by a mutable borrow, ie by a call to `.unsafe_index_mut()` with the same index.
    #[inline]
    unsafe fn unsafe_index(&self, index: usize) -> &Self::Output {
        unsafe { self.data.unsafe_index(index) }
    }
    /// # Safety
    /// The caller must ensure that the returned reference is not aliased by another borrow, ie by a call to `.unsafe_index()` or `.unsafe_index_mut()` with the same index.
    #[inline]
    unsafe fn unsafe_index_mut(&self, index: usize) -> &mut Self::Output {
        unsafe { self.data.unsafe_index_mut(index) }
    }
}

impl<'a, T: Scalar> Slice<'a, T> {
    // For kernel macro.
    #[doc(hidden)]
    #[cfg(target_arch = "spirv")]
    #[inline]
    pub unsafe fn from_raw_parts(inner: &'a [T; 1], offset: usize, len: usize) -> Self {
        let data = SliceRepr { inner, offset, len };
        Self { data }
    }
    /// A pointer to the buffer's data.
    #[cfg(not(target_arch = "spirv"))]
    #[inline]
    pub fn as_ptr(&self) -> *const T {
        self.data.inner.as_ptr()
    }
}

impl<'a, T: Scalar> UnsafeSlice<'a, T> {
    // For kernel macro.
    #[doc(hidden)]
    #[cfg(target_arch = "spirv")]
    #[inline]
    pub unsafe fn from_unsafe_raw_parts(inner: &'a [T; 1], offset: usize, len: usize) -> Self {
        let data = UnsafeSliceRepr {
            inner: &*inner,
            offset,
            len,
        };
        Self { data }
    }
    /// A mutable pointer to the buffer's data.
    #[cfg(not(target_arch = "spirv"))]
    #[inline]
    pub fn as_mut_ptr(&self) -> *mut T {
        self.data.ptr
    }
}

#[cfg(not(target_arch = "spirv"))]
impl<'a, T: Scalar> From<&'a [T]> for Slice<'a, T> {
    #[inline]
    fn from(slice: &'a [T]) -> Self {
        let data = SliceRepr { inner: slice };
        Self { data }
    }
}

#[cfg(not(target_arch = "spirv"))]
impl<'a, T: Scalar> From<Slice<'a, T>> for &'a [T] {
    #[inline]
    fn from(slice: Slice<'a, T>) -> &'a [T] {
        slice.data.inner
    }
}

#[cfg(not(target_arch = "spirv"))]
impl<'a, T: Scalar> From<&'a mut [T]> for UnsafeSlice<'a, T> {
    #[inline]
    fn from(slice: &'a mut [T]) -> Self {
        let data = UnsafeSliceRepr {
            ptr: slice.as_mut_ptr(),
            len: slice.len(),
            _m: PhantomData,
        };
        Self { data }
    }
}
