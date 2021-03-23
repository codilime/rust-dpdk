use crate::ffi;

lazy_static! {
    pub static ref RTE_RING_NAMESIZE: usize = ffi::RTE_MEMZONE_NAMESIZE as usize - ffi::RTE_RING_MZ_PREFIX.len() + 1;
}
