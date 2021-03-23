// mod config;
pub mod eal;
pub mod launch;
pub mod lcore;
pub mod memory;
pub mod memzone;
#[macro_use]
pub mod malloc;
pub mod dev;
#[macro_use]
pub mod byteorder;
mod cycles;

// pub use self::config::{config, Config, MemoryConfig};
pub use self::lcore::{socket_count, socket_id};
pub use self::cycles::*;
