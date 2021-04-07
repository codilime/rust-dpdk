#![warn(rust_2018_idioms)]

//! Rust binding for DPDK
//!
//! Currently, build.rs cannot configure linker options, thus, a user must set RUSTFLAGS env
//! variable as this library's panic message says.

#[allow(warnings, clippy)]
mod dpdk;
pub use dpdk::*;

#[link(name = "bsd")]
extern "C" {}

#[link(name = "pcap")]
extern "C" {}

include!(concat!(env!("OUT_DIR"), "/lib.rs"));

/// Thin compatibility layer for items which names changed between dpdk 19 and 20 versions. This
/// allows dowstream crates to compile on both versions.
pub mod compat {
    #[cfg(main_lcore_name = "main")]
    pub use super::rte_get_main_lcore;
    #[cfg(main_lcore_name = "master")]
    pub use super::rte_get_master_lcore as rte_get_main_lcore;

    #[cfg(main_lcore_name = "main")]
    pub use super::rte_rmt_call_main_t_SKIP_MAIN;
    #[cfg(main_lcore_name = "master")]
    pub use super::rte_rmt_call_master_t_SKIP_MASTER as rte_rmt_call_main_t_SKIP_MAIN;

    #[cfg(main_lcore_name = "main")]
    pub use super::rte_rmt_call_main_t_CALL_MAIN;
    #[cfg(main_lcore_name = "master")]
    pub use super::rte_rmt_call_master_t_CALL_MASTER as rte_rmt_call_main_t_CALL_MAIN;
}
