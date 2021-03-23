use std::mem;
use std::os::raw::c_void;
// use std::os::unix::io::AsRawFd;
use std::ptr;

use cfile;

use crate::ffi;

#[macro_export]
macro_rules! rte_new {
    ($t:ty) => {
        unsafe {
            ::std::mem::transmute($crate::malloc::zmalloc(
                stringify!($t),
                ::std::mem::size_of::<$t>(),
                $crate::RTE_CACHE_LINE_SIZE,
            ) as *mut $t)
        }
    };
}

#[macro_export]
macro_rules! rte_new_array {
    ($t:ty; $num:expr) => {
        unsafe {
            ::std::mem::transmute(::std::slice::from_raw_parts_mut(
                $crate::malloc::calloc(
                    stringify!($t),
                    $num,
                    ::std::mem::size_of::<$t>(),
                    $crate::RTE_CACHE_LINE_SIZE,
                ) as *mut $t,
                $num,
            ))
        }
    };
}

#[macro_export]
macro_rules! rte_free {
    ($p:expr) => {
        $crate::malloc::free($p as *mut ::std::os::raw::c_void)
    };
}

/// This function allocates memory from the huge-page area of memory.
///
/// The memory is not cleared. In NUMA systems, the memory allocated
/// resides on the same NUMA socket as the core that calls this function.
///
pub fn malloc(tag: &'static str, size: usize, align: u32) -> *mut c_void {
    unsafe { ffi::rte_malloc(tag.as_ptr() as *const i8, size, align) }
}

/// Allocate zero'ed memory from the heap.
///
/// Equivalent to rte_malloc() except that the memory zone is initialised with zeros.
/// In NUMA systems, the memory allocated resides on the same NUMA socket
/// as the core that calls this function.
///
pub fn zmalloc(tag: &'static str, size: usize, align: u32) -> *mut c_void {
    unsafe { ffi::rte_zmalloc(tag.as_ptr() as *const i8, size, align) }
}

/// Replacement function for calloc(), using huge-page memory.
///
/// Memory area is initialised with zeros. In NUMA systems,
/// the memory allocated resides on the same NUMA socket as the core that calls this function.
///
pub fn calloc(tag: &'static str, num: usize, size: usize, align: u32) -> *mut c_void {
    unsafe { ffi::rte_calloc(tag.as_ptr() as *const i8, num, size, align) }
}

/// Replacement function for realloc(), using huge-page memory.
///
/// Reserved area memory is resized, preserving contents.
/// In NUMA systems, the new area resides on the same NUMA socket as the old area.
///
pub fn realloc(ptr: *mut c_void, size: usize, align: u32) -> *mut c_void {
    unsafe { ffi::rte_realloc(ptr, size, align) }
}

/// This function allocates memory from the huge-page area of memory.
///
/// The memory is not cleared.
///
pub fn malloc_socket(tag: &'static str, size: usize, align: u32, socket_id: i32) -> *mut c_void {
    unsafe { ffi::rte_malloc_socket(tag.as_ptr() as *const i8, size, align, socket_id) }
}

/// Allocate zero'ed memory from the heap.
///
/// Equivalent to rte_malloc() except that the memory zone is initialised with zeros.
///
pub fn zmalloc_socket(tag: &'static str, size: usize, align: u32, socket_id: i32) -> *mut c_void {
    unsafe { ffi::rte_zmalloc_socket(tag.as_ptr() as *const i8, size, align, socket_id) }
}

/// Replacement function for calloc(), using huge-page memory.
///
/// Memory area is initialised with zeros.
///
pub fn calloc_socket(tag: &'static str, num: usize, size: usize, align: u32, socket_id: i32) -> *mut c_void {
    unsafe { ffi::rte_calloc_socket(tag.as_ptr() as *const i8, num, size, align, socket_id) }
}

/// Frees the memory space pointed to by the provided pointer.
pub fn free(ptr: *mut c_void) {
    unsafe { ffi::rte_free(ptr as *mut c_void) }
}

/// Get heap statistics for the specified heap.
pub fn get_socket_stats(socket_id: i32) -> Option<ffi::rte_malloc_socket_stats> {
    unsafe {
        let mut stats: ffi::rte_malloc_socket_stats = mem::zeroed();

        if ffi::rte_malloc_get_socket_stats(socket_id, &mut stats) == 0 {
            Some(stats)
        } else {
            None
        }
    }
}

// /// Dump statistics.
// pub fn dump_stats<S: AsRawFd>(s: &S, tag: Option<&str>) {
//     if let Ok(mut f) = cfile::fdopen(s, "w") {
//         unsafe {
//             ffi::rte_malloc_dump_stats(
//                 &mut **f as *mut _ as *mut _,
//                 tag.map_or_else(ptr::null, |s| s.as_ptr() as *const i8),
//             );
//         }
//     }
// }
