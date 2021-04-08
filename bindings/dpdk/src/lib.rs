#![warn(rust_2018_idioms)]

mod ffi;

pub mod eal;

/// Reexport of crossbeam's [thread][crossbeam_utils::thread] module
///
/// This is reexported so that downstream crates don't have to manually import crossbeam and won't
/// have version conflicts
pub use crossbeam_utils::thread as thread;
