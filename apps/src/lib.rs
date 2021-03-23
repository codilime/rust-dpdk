#![allow(
    deprecated,
    unused,
    clippy::useless_attribute,
    clippy::not_unsafe_ptr_arg_deref,
    clippy::trivially_copy_pass_by_ref,
    clippy::many_single_char_names
)]

#[macro_use]
extern crate log;
#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate failure;
#[macro_use]
extern crate lazy_static;
extern crate cfile;
extern crate errno;
extern crate itertools;
extern crate libc;
extern crate rand;
extern crate time;
#[macro_use]
extern crate num_derive;
extern crate num_traits;

extern crate dpdk_sys;

pub mod ffi;

#[macro_use]
pub mod macros;
#[macro_use]
pub mod errors;
#[macro_use]
mod common;
#[macro_use]
pub mod utils;
pub mod ring;
pub mod mempool;
pub mod mbuf;
pub mod ether;
pub mod ethdev;

pub use self::common::*;
