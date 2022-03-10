#![no_std]

use core::marker::PhantomData;
use core::slice;

use coca::{
    collections::vec::Vec,
    storage::{self, ArrayLayout, Capacity, DefaultStorage, OwnedStorage, Storage},
    CapacityError,
};
use zeroize::{Zeroize, ZeroizeOnDrop};

pub struct ZArrayStorage<Z: Zeroize, S: Storage<ArrayLayout<Z>>>(S, PhantomData<Z>);

impl<Z: Zeroize, S: Storage<ArrayLayout<Z>>> From<S> for ZArrayStorage<Z, S> {
    #[inline]
    fn from(s: S) -> Self {
        Self(s, PhantomData)
    }
}

unsafe impl<Z: Zeroize, S: Storage<ArrayLayout<Z>>> Storage<ArrayLayout<Z>>
    for ZArrayStorage<Z, S>
{
    const MIN_REPRESENTABLE: usize = S::MIN_REPRESENTABLE;

    #[inline]
    fn get_ptr(&self) -> *const u8 {
        self.0.get_ptr()
    }
    #[inline]
    fn get_mut_ptr(&mut self) -> *mut u8 {
        self.0.get_mut_ptr()
    }
    #[inline]
    fn capacity(&self) -> usize {
        self.0.capacity()
    }
    #[inline]
    fn try_grow<I: Capacity>(&self, min_capacity: Option<usize>) -> Result<Self, CapacityError> {
        Ok(Self(self.0.try_grow::<I>(min_capacity)?, PhantomData))
    }
}

impl<Z: Zeroize, S: OwnedStorage<ArrayLayout<Z>>> OwnedStorage<ArrayLayout<Z>>
    for ZArrayStorage<Z, S>
{
    #[inline]
    fn try_with_capacity(min_capacity: usize) -> Result<Self, CapacityError> {
        Ok(S::try_with_capacity(min_capacity)?.into())
    }
}

impl<Z: Zeroize, S: DefaultStorage<ArrayLayout<Z>>> DefaultStorage<ArrayLayout<Z>>
    for ZArrayStorage<Z, S>
{
    const UNINIT: Self = Self(S::UNINIT, PhantomData);
}

impl<Z: Zeroize, S: Storage<ArrayLayout<Z>>> Zeroize for ZArrayStorage<Z, S> {
    fn zeroize(&mut self) {
        let uninit_slice =
            unsafe { slice::from_raw_parts_mut(self.0.get_mut_ptr(), self.0.capacity()) };
        uninit_slice.zeroize();
    }
}

impl<Z: Zeroize, S: Storage<ArrayLayout<Z>>> Drop for ZArrayStorage<Z, S> {
    fn drop(&mut self) {
        self.zeroize();
    }
}

impl<Z: Zeroize, S: Storage<ArrayLayout<Z>>> ZeroizeOnDrop for ZArrayStorage<Z, S> {}

pub type ZInlineStorage<Z, const N: usize> = ZArrayStorage<Z, storage::InlineStorage<Z, N>>;

pub type ZSliceStorage<'s, Z> = ZArrayStorage<Z, storage::SliceStorage<'s, Z>>;

pub type ZArenaStorage<'s, Z> = ZArrayStorage<Z, storage::ArenaStorage<'s, Z>>;

pub type ZInlineVec<T, const N: usize, I = usize> = Vec<T, ZInlineStorage<T, N>, I>;

pub type ZSliceVec<'s, T, I = usize> = Vec<T, ZSliceStorage<'s, T>, I>;

pub type ZArenaVec<'s, T, I = usize> = Vec<T, ZArenaStorage<'s, T>, I>;

#[cfg(feature = "alloc")]
pub type ZAllocVec<T, I = usize> =
    Vec<T, ZArrayStorage<T, storage::AllocStorage<ArrayLayout<T>>>, I>;

#[cfg(feature = "alloc")]
pub type ZReallocVec<T, I = usize> =
    Vec<T, ZArrayStorage<T, storage::ReallocStorage<ArrayLayout<T>>>, I>;

#[cfg(test)]
mod tests {
    use core::mem::MaybeUninit;

    use super::*;

    #[test]
    fn create_inline() {
        let mut z = ZInlineVec::<u8, 3>::new();
        z.push(1);
        z.push(2);
        z.push(3);
        assert!(z.try_push(4).is_err());
    }

    #[test]
    fn zeroize_inline() {
        let buf: [MaybeUninit<u8>; 3] = [
            MaybeUninit::new(1),
            MaybeUninit::new(2),
            MaybeUninit::new(3),
        ];
        let mut vec = unsafe { ZInlineVec::<u8, 3>::from_raw_parts(buf.into(), 1) };
        vec[0] = 10;
        assert_eq!(vec[0], 10);
        let (mut stor, len) = vec.into_raw_parts();
        assert_eq!(len, 1);
        assert_eq!(stor.capacity(), 3);
        let vals = unsafe { (stor.get_ptr().cast::<[u8; 3]>()).read() };
        assert_eq!(vals, [10, 2, 3]);
        stor.zeroize();
        let vals = unsafe { (stor.get_ptr().cast::<[u8; 3]>()).read() };
        assert_eq!(vals, [0, 0, 0]);
    }

    #[test]
    fn create_slice() {
        let mut buf: [MaybeUninit<u8>; 3] = [
            MaybeUninit::uninit(),
            MaybeUninit::uninit(),
            MaybeUninit::uninit(),
        ];
        let mut z = ZSliceVec::<u8>::from(ZSliceStorage::from(&mut buf[..]));
        z.push(1);
        z.push(2);
        z.push(3);
        assert!(z.try_push(4).is_err());
    }

    #[test]
    fn zeroize_slice() {
        let mut buf: [MaybeUninit<u8>; 3] = [
            MaybeUninit::new(1),
            MaybeUninit::new(2),
            MaybeUninit::new(3),
        ];
        let mut vec =
            unsafe { ZSliceVec::<u8>::from_raw_parts(ZSliceStorage::from(&mut buf[..]), 1) };
        vec[0] = 10;
        assert_eq!(vec[0], 10);
        drop(vec);
        let vals = unsafe { (buf.get_ptr().cast::<[u8; 3]>()).read() };
        assert_eq!(vals, [0, 0, 0]);
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn zeroize_realloc() {
        let mut vec = ZReallocVec::<u8>::new();
        vec.push(1);
        vec.push(2);
        vec.push(3);
        assert_eq!(vec.pop(), Some(3));
        let (mut stor, len) = vec.into_raw_parts();
        assert_eq!(len, 2);
        assert!(stor.capacity() > 2);
        let vals = unsafe { (stor.get_ptr().cast::<[u8; 3]>()).read() };
        assert_eq!(vals, [1, 2, 3]);
        stor.zeroize();
        let vals = unsafe { (stor.get_ptr().cast::<[u8; 3]>()).read() };
        assert_eq!(vals, [0, 0, 0]);
    }
}
