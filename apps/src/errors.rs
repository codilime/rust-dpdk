use std::ffi::CStr;
use std::fmt;
use std::os::raw::c_int;
use std::ptr::NonNull;
use std::result;

use errno::errno;
use failure::{Error, Fail};

use crate::ffi;

pub type Result<T> = result::Result<T, failure::Error>;

pub trait AsResult {
    type Result;

    fn as_result(self) -> Result<Self::Result>;

    fn ok_or<E: Fail>(self, err: E) -> Result<Self::Result>;

    fn ok_or_else<E: Fail, F: FnOnce() -> E>(self, err: F) -> Result<Self::Result>;
}

impl<T> AsResult for *mut T {
    type Result = NonNull<T>;

    fn as_result(self) -> Result<Self::Result> {
        NonNull::new(self).ok_or_else(rte_error)
    }

    fn ok_or<E: Fail>(self, err: E) -> Result<Self::Result> {
        NonNull::new(self).ok_or_else(|| err.into())
    }

    fn ok_or_else<E: Fail, F: FnOnce() -> E>(self, err: F) -> Result<Self::Result> {
        NonNull::new(self).ok_or_else(|| err().into())
    }
}

impl AsResult for c_int {
    type Result = c_int;

    fn as_result(self) -> Result<Self::Result> {
        if self == -1 {
            Err(RteError(self).into())
        } else {
            Ok(self)
        }
    }

    fn ok_or<E: Fail>(self, err: E) -> Result<Self::Result> {
        if self == -1 {
            Err(err.into())
        } else {
            Ok(self)
        }
    }

    fn ok_or_else<E: Fail, F: FnOnce() -> E>(self, err: F) -> Result<Self::Result> {
        if self == -1 {
            Err(err().into())
        } else {
            Ok(self)
        }
    }
}

macro_rules! rte_check {
    ( $ret:expr ) => {
        rte_check!($ret; ok => {()}; err => {$crate::errors::RteError($ret).into()})
    };
    ( $ret:expr; ok => $ok:block) => {
        rte_check!($ret; ok => $ok; err => {$crate::errors::RteError($ret).into()})
    };
    ( $ret:expr; err => $err:block) => {
        rte_check!($ret; ok => {()}; err => $err)
    };
    ( $ret:expr; ok => $ok:block; err => $err:block ) => {{
        if $ret == 0 {
            Ok($ok)
        } else {
            Err($err)
        }
    }};

    ( $ret:expr, NonNull ) => {
        rte_check!($ret, NonNull; ok => {$ret}; err => {$crate::errors::rte_error()})
    };
    ( $ret:expr, NonNull; ok => $ok:block) => {
        rte_check!($ret, NonNull; ok => $ok; err => {$crate::errors::rte_error()})
    };
    ( $ret:expr, NonNull; err => $err:block) => {
        rte_check!($ret, NonNull; ok => {$ret}; err => $err)
    };
    ( $ret:expr, NonNull; ok => $ok:block; err => $err:block ) => {{
        if !$ret.is_null() {
            Ok($ok)
        } else {
            Err($err)
        }
    }};
}

#[derive(Debug, Fail)]
pub struct RteError(pub i32);

impl fmt::Display for RteError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "RTE error, {} ({})",
            unsafe { CStr::from_ptr(ffi::rte_strerror(self.0)).to_string_lossy() },
            self.0
        )
    }
}

#[derive(Debug, Fail)]
pub enum ErrorKind {
    #[fail(display = "invalid log type, {}", _0)]
    InvalidLogType(u32),
    #[fail(display = "invalid log level, {}", _0)]
    InvalidLogLevel(u32),
    #[fail(display = "cmdline parse error, {}", _0)]
    CmdLineParseError(i32),
    #[fail(display = "{}", _0)]
    OsError(i32),
}

pub fn rte_error() -> Error {
    RteError(unsafe { ffi::rte_errno() }).into()
}

pub fn os_error() -> Error {
    ErrorKind::OsError(errno().0 as i32).into()
}
