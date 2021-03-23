use crate::ffi;

/// RTE Memzone
///
/// The goal of the memzone allocator is to reserve contiguous portions of physical memory.
/// These zones are identified by a name.
///
/// The memzone descriptors are shared by all partitions
/// and are located in a known place of physical memory.
/// This zone is accessed using rte_eal_get_configuration().
/// The lookup (by name) of a memory zone can be done
// in any partition and returns the same physical address.
///
/// A reserved memory zone cannot be unreserved.
/// The reservation shall be done at initialization time only.
///
pub struct MemoryZone(*const ffi::rte_memzone);

pub fn from_raw(zone: *const ffi::rte_memzone) -> MemoryZone {
    MemoryZone(zone)
}
