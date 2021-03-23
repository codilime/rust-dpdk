use std::borrow::Borrow;
use std::ffi::CString;
use std::ops::{Deref, DerefMut};
use std::ptr;

pub trait Raw<T>: Deref<Target = T> + DerefMut + AsRaw<Raw = T> + IntoRaw + FromRaw + From<*mut T> {}

pub trait AsRaw {
    type Raw;

    fn as_raw(&self) -> *mut Self::Raw;
}

impl<T: AsRaw> AsRaw for &T {
    type Raw = T::Raw;

    fn as_raw(&self) -> *mut Self::Raw {
        (*self).as_raw()
    }
}

impl<T: AsRaw> AsRaw for Option<T> {
    type Raw = T::Raw;

    fn as_raw(&self) -> *mut Self::Raw {
        self.as_ref().map(|p| p.as_raw()).unwrap_or(ptr::null_mut())
    }
}

pub trait IntoRaw: AsRaw {
    fn into_raw(self) -> *mut Self::Raw;
}

pub trait FromRaw: AsRaw
where
    Self: Sized,
{
    fn from_raw(raw: *mut Self::Raw) -> Option<Self>;
}

macro_rules! raw {
    (pub $wrapper:ident ( $raw_ty:ty ) ) => {
        #[repr(transparent)]
        #[derive(Debug)]
        pub struct $wrapper(::std::ptr::NonNull<$raw_ty>);

        impl $crate::utils::Raw<$raw_ty> for $wrapper {}

        impl ::std::ops::Deref for $wrapper {
            type Target = $raw_ty;

            fn deref(&self) -> &Self::Target {
                unsafe { self.0.as_ref() }
            }
        }

        impl ::std::ops::DerefMut for $wrapper {
            fn deref_mut(&mut self) -> &mut Self::Target {
                unsafe { self.0.as_mut() }
            }
        }

        impl $crate::utils::AsRaw for $wrapper {
            type Raw = $raw_ty;

            fn as_raw(&self) -> *mut Self::Raw {
                self.0.as_ptr()
            }
        }

        impl $crate::utils::IntoRaw for $wrapper {
            fn into_raw(self) -> *mut Self::Raw {
                self.0.as_ptr()
            }
        }

        impl $crate::utils::FromRaw for $wrapper {
            fn from_raw(raw: *mut Self::Raw) -> Option<Self> {
                ::std::ptr::NonNull::new(raw).map($wrapper)
            }
        }

        impl From<*mut $raw_ty> for $wrapper {
            fn from(p: *mut $raw_ty) -> Self {
                use $crate::utils::FromRaw;

                Self::from_raw(p).unwrap()
            }
        }
    };
}

pub trait AsCString {
    fn as_cstring(&self) -> CString;
}

impl<T> AsCString for T
where
    T: AsRef<str>,
{
    fn as_cstring(&self) -> CString {
        let mut v = self.as_ref().as_bytes().to_owned();
        v.push(0);
        unsafe { CString::from_vec_unchecked(v) }
    }
}

pub struct CallbackContext<F, T> {
    pub callback: F,
    pub arg: T,
}

impl<F, T> CallbackContext<F, T> {
    pub fn new(callback: F, arg: T) -> Self {
        CallbackContext { callback, arg }
    }

    pub fn into_raw<R>(self) -> *mut R {
        Box::into_raw(Box::new(self)) as *mut _
    }

    pub fn from_raw<V>(raw: *mut V) -> Box<Self> {
        unsafe { Box::from_raw(raw as *mut _) }
    }
}
