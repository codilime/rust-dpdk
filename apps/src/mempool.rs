//!
//! RTE Mempool.
//!
//! A memory pool is an allocator of fixed-size object. It is
//! identified by its name, and uses a ring to store free objects. It
//! provides some other optional services, like a per-core object
//! cache, and an alignment helper to ensure that objects are padded
//! to spread them equally on all RAM channels, ranks, and so on.
//!
//! Objects owned by a mempool should never be added in another
//! mempool. When an object is freed using rte_mempool_put() or
//! equivalent, the object data is not modified; the user can save some
//! meta-data in the object data and retrieve them when allocating a
//! new object.
//!
//! Note: the mempool implementation is not preemptible. An lcore must not be
//! interrupted by another task that uses the same mempool (because it uses a
//! ring which is not preemptible). Also, usual mempool functions like
//! rte_mempool_get() or rte_mempool_put() are designed to be called from an EAL
//! thread due to the internal per-lcore cache. Due to the lack of caching,
//! rte_mempool_get() or rte_mempool_put() performance will suffer when called
//! by non-EAL threads. Instead, non-EAL threads should call
//! rte_mempool_generic_get() or rte_mempool_generic_put() with a user cache
//! created with rte_mempool_cache_create().
//!
use std::ffi::CStr;
use std::mem;
use std::os::raw::{c_uint, c_void};
// use std::os::unix::io::AsRawFd;
use std::ptr::{self, NonNull};

use cfile;
use crate::ffi;
use libc;

use crate::errors::{AsResult, Result};
use crate::common::lcore;
use crate::memory::SocketId;
use crate::ring;
use crate::utils::{AsCString, AsRaw, CallbackContext, FromRaw, IntoRaw, Raw};

// pub use crate::ffi::{
//     MEMPOOL_PG_NUM_DEFAULT, RTE_MEMPOOL_ALIGN, RTE_MEMPOOL_ALIGN_MASK, RTE_MEMPOOL_HEADER_COOKIE1,
//     RTE_MEMPOOL_HEADER_COOKIE2, RTE_MEMPOOL_MZ_FORMAT, RTE_MEMPOOL_MZ_PREFIX, RTE_MEMPOOL_TRAILER_COOKIE,
// };

lazy_static! {
    pub static ref RTE_MEMPOOL_NAMESIZE: usize = *ring::RTE_RING_NAMESIZE - ffi::RTE_MEMPOOL_MZ_PREFIX.len() + 1;
}

bitflags! {
    pub struct MemoryPoolFlags: u32 {
        /// Do not spread in memory.
        const MEMPOOL_F_NO_SPREAD       = ffi::MEMPOOL_F_NO_SPREAD;
        /// Do not align objs on cache lines.
        const MEMPOOL_F_NO_CACHE_ALIGN  = ffi::MEMPOOL_F_NO_CACHE_ALIGN;
        /// Default put is "single-producer".
        const MEMPOOL_F_SP_PUT          = ffi::MEMPOOL_F_SP_PUT;
        /// Default get is "single-consumer".
        const MEMPOOL_F_SC_GET          = ffi::MEMPOOL_F_SC_GET;
        /// Internal: pool is created.
        const MEMPOOL_F_POOL_CREATED    = ffi::MEMPOOL_F_POOL_CREATED;
        /// Don't need IOVA contiguous objs.
        const MEMPOOL_F_NO_IOVA_CONTIG  = ffi::MEMPOOL_F_NO_IOVA_CONTIG;
    }
}

pub trait Pooled<T>: Raw<T> {
    /// Return a pointer to the mempool owning this object.
    fn pool(&self) -> MemoryPool {
        unsafe { ffi::rte_mempool_from_obj(self.as_raw() as *mut _) }.into()
    }

    /// Return the IO address of elt, which is an element of the pool mp.
    fn virt2iova(&self) -> ffi::rte_iova_t {
        unsafe { ffi::rte_mempool_virt2iova(self.as_raw() as *mut _ as *const _) }
    }
}

/// A mempool constructor callback function.
pub type Constructor<T> = fn(pool: &MemoryPool, arg: Option<T>);

/// A mempool walk callback function.
pub type PoolWalkCallback<T> = fn(pool: &MemoryPool, arg: Option<T>);

/// A mempool object iterator callback function.
pub type ObjectCallback<T, O> = fn(pool: &MemoryPool, arg: Option<T>, obj: &mut O, idx: usize);

pub type MemoryChunkCallback<T> = fn(pool: &MemoryPool, arg: Option<T>, mem: &ffi::rte_mempool_memhdr, idx: usize);

pub type RawMemoryPool = ffi::rte_mempool;
pub type RawMemoryPoolPtr = *mut ffi::rte_mempool;

/// The RTE mempool structure.
raw!(pub MemoryPool(RawMemoryPool));

impl MemoryPool {
    /// Search a mempool from its name
    pub fn lookup<S: AsRef<str>>(name: S) -> Result<Self> {
        let name = name.as_cstring();

        unsafe { ffi::rte_mempool_lookup(name.as_ptr()) }
            .as_result()
            .map(MemoryPool)
    }

    /// Name of mempool.
    pub fn name(&self) -> &str {
        unsafe { CStr::from_ptr((&self.name[..]).as_ptr()).to_str().unwrap() }
    }

    /// Free a mempool
    ///
    /// Unlink the mempool from global list, free the memory chunks, and all
    /// memory referenced by the mempool. The objects must not be used by
    /// other cores as they will be freed.
    fn free(&mut self) {
        unsafe { ffi::rte_mempool_free(self.as_raw()) }
    }

    /// Return the number of entries in the mempool.
    ///
    /// When cache is enabled, this function has to browse the length of
    /// all lcores, so it should not be used in a data path, but only for
    /// debug purposes. User-owned mempool caches are not accounted for.
    pub fn avail_count(&self) -> usize {
        unsafe { ffi::rte_mempool_avail_count(self.as_raw()) as usize }
    }

    /// Return the number of elements which have been allocated from the mempool
    ///
    /// When cache is enabled, this function has to browse the length of
    /// all lcores, so it should not be used in a data path, but only for
    /// debug purposes.
    pub fn in_use_count(&self) -> usize {
        unsafe { ffi::rte_mempool_in_use_count(self.as_raw()) as usize }
    }

    /// Test if the mempool is full.
    pub fn is_full(&self) -> bool {
        self.avail_count() == self.size as usize
    }

    /// Test if the mempool is empty.
    pub fn is_empty(&self) -> bool {
        self.avail_count() == 0
    }

    /// Check the consistency of mempool objects.
    ///
    /// Verify the coherency of fields in the mempool structure.
    /// Also check that the cookies of mempool objects (even the ones that are not present in pool)
    /// have a correct value. If not, a panic will occur.
    ///
    pub fn audit(&self) {
        unsafe { ffi::rte_mempool_audit(self.as_raw()) }
    }

    /// Return a pointer to the private data in an mempool structure.
    pub fn get_priv<T>(&self) -> *const T {
        unsafe { ffi::rte_mempool_get_priv(self.as_raw()) as *const _ }
    }

    // /// Dump the status of the mempool to the console.
    // pub fn dump<S: AsRawFd>(&self, s: &S) -> Result<()> {
    //     let mut f = cfile::fdopen(s, "w")?;

    //     unsafe { ffi::rte_mempool_dump(&mut **f as *mut _ as *mut _, self.as_raw()) };

    //     Ok(())
    // }

    // /// Dump the status of all mempools on the console
    // pub fn list_dump<S: AsRawFd>(s: &S) -> Result<()> {
    //     let mut f = cfile::fdopen(s, "w")?;

    //     unsafe { ffi::rte_mempool_list_dump(&mut **f as *mut _ as *mut _) };

    //     Ok(())
    // }

    /// Call a function for each mempool object in a memory chunk
    ///
    /// Iterate across objects of the given size and alignment in the provided chunk of memory.
    /// The given memory buffer can consist of disjointed physical pages.
    ///
    /// For each object, call the provided callback (if any).
    /// This function is used to populate a mempool, or walk through all the elements of a mempool,
    /// or estimate how many elements of the given size could be created in the given memory buffer.
    ///
    pub fn walk<T, O>(&mut self, callback: ObjectCallback<T, O>, arg: Option<T>) -> usize {
        unsafe {
            ffi::rte_mempool_obj_iter(
                self.as_raw(),
                Some(obj_cb_stub::<T, O>),
                ObjectContext::new(callback, arg).into_raw(),
            ) as usize
        }
    }
}

/// Create a new mempool named name in memory.
///
/// This function uses memzone_reserve() to allocate memory.
/// The pool contains n elements of elt_size. Its size is set to n.
/// All elements of the mempool are allocated together with the mempool header,
/// in one physically continuous chunk of memory.
///
pub fn create<S, M, T, O>(
    name: S,
    n: u32,
    cache_size: u32,
    private_data_size: u32,
    mp_init: Option<Constructor<M>>,
    mp_init_arg: Option<M>,
    obj_init: Option<ObjectCallback<T, O>>,
    obj_init_arg: Option<T>,
    socket_id: SocketId,
    flags: MemoryPoolFlags,
) -> Result<MemoryPool>
where
    S: AsRef<str>,
{
    let name = name.as_cstring();

    let mp_init_ctx = if let Some(callback) = mp_init {
        ConstructorContext::new(callback, mp_init_arg).into_raw()
    } else {
        ptr::null_mut()
    };
    let obj_init_ctx = if let Some(callback) = obj_init {
        ObjectContext::new(callback, obj_init_arg).into_raw()
    } else {
        ptr::null_mut()
    };

    unsafe {
        ffi::rte_mempool_create(
            name.as_ptr(),
            n,
            mem::size_of::<O>() as u32,
            cache_size,
            private_data_size,
            if mp_init.is_none() {
                None
            } else {
                Some(mp_init_stub::<T>)
            },
            mp_init_ctx,
            if obj_init.is_none() {
                None
            } else {
                Some(obj_cb_stub::<T, O>)
            },
            obj_init_ctx,
            socket_id,
            flags.bits,
        )
    }
    .as_result()
    .map(MemoryPool)
}

/// Create an empty mempool
///
/// The mempool is allocated and initialized, but it is not populated:
/// no memory is allocated for the mempool elements.
/// The user has to call rte_mempool_populate_*() to add memory chunks to the pool.
/// Once populated, the user may also want to initialize each object with rte_mempool_obj_iter().
pub fn create_empty<S, O>(
    name: S,
    n: u32,
    cache_size: u32,
    private_data_size: u32,
    socket_id: SocketId,
    flags: MemoryPoolFlags,
) -> Result<MemoryPool>
where
    S: AsRef<str>,
{
    let name = name.as_cstring();

    unsafe {
        ffi::rte_mempool_create_empty(
            name.as_ptr(),
            n,
            mem::size_of::<O>() as u32,
            cache_size,
            private_data_size,
            socket_id,
            flags.bits,
        )
    }
    .as_result()
    .map(MemoryPool)
}

type ConstructorContext<T> = CallbackContext<Constructor<T>, Option<T>>;

unsafe extern "C" fn mp_init_stub<T>(mp: *mut ffi::rte_mempool, arg: *mut c_void) {
    let mp = MemoryPool::from(mp);
    let ctx = ConstructorContext::<T>::from_raw(arg);

    (ctx.callback)(&mp, ctx.arg);

    mem::forget(mp);
}

type ObjectContext<T, O> = CallbackContext<ObjectCallback<T, O>, Option<T>>;

unsafe extern "C" fn obj_cb_stub<T, O>(mp: *mut ffi::rte_mempool, arg: *mut c_void, obj: *mut c_void, obj_idx: c_uint) {
    let mp = MemoryPool::from(mp);
    let ctx = ObjectContext::<T, O>::from_raw(arg);

    (ctx.callback)(&mp, ctx.arg, (obj as *mut O).as_mut().unwrap(), obj_idx as usize);

    mem::forget(mp);
}

type MemoryChunkContext<T> = CallbackContext<MemoryChunkCallback<T>, Option<T>>;

unsafe extern "C" fn mem_cb_stub<T>(
    mp: *mut ffi::rte_mempool,
    arg: *mut c_void,
    memhdr: *mut ffi::rte_mempool_memhdr,
    mem_idx: c_uint,
) {
    let mp = MemoryPool::from(mp);
    let ctx = MemoryChunkContext::<T>::from_raw(arg);

    (ctx.callback)(&mp, ctx.arg, &*memhdr, mem_idx as usize);

    mem::forget(mp);
}

pub fn lookup(name: &str) -> Result<RawMemoryPoolPtr> {
    let p = unsafe { ffi::rte_mempool_lookup(r#try!(to_cptr!(name))) };

    rte_check!(p, NonNull)
}

// /// Dump the status of all mempools on the console
// pub fn list_dump<S: AsRawFd>(s: &S) {
//     if let Ok(mut f) = cfile::fdopen(s, "w") {
//         unsafe {
//             ffi::rte_mempool_list_dump(&mut **f as *mut _ as *mut _);
//         }
//     }
// }

/// Walk list of all memory pools
pub fn walk<T>(callback: PoolWalkCallback<T>, arg: Option<T>) {
    unsafe {
        ffi::rte_mempool_walk(
            Some(pool_walk_stub::<T>),
            PoolWalkContext::new(callback, arg).into_raw(),
        );
    }
}

type PoolWalkContext<T> = CallbackContext<PoolWalkCallback<T>, Option<T>>;

unsafe extern "C" fn pool_walk_stub<T>(mp: *mut ffi::rte_mempool, arg: *mut libc::c_void) {
    let mp = MemoryPool::from(mp);
    let ctxt = PoolWalkContext::<T>::from_raw(arg);

    (ctxt.callback)(&mp, ctxt.arg);

    mem::forget(mp)
}

pub type RawCache = ffi::rte_mempool_cache;
pub type RawCachePtr = *mut ffi::rte_mempool_cache;

raw!(pub Cache(RawCache));

impl Cache {
    /// Create a user-owned mempool cache.
    ///
    /// This can be used by non-EAL threads to enable caching
    /// when they interact with a mempool.
    pub fn create(size: usize, socket_id: SocketId) -> Self {
        unsafe { ffi::rte_mempool_cache_create(size as u32, socket_id) }.into()
    }

    /// Free a user-owned mempool cache.
    fn free(self) {
        unsafe { ffi::rte_mempool_cache_free(self.as_raw()) }
    }
}

impl MemoryPool {
    /// Flush a user-owned mempool cache to the specified mempool.
    pub fn flush(&self, cache: &Cache) {
        unsafe { ffi::rte_mempool_cache_flush(cache.as_raw(), self.as_raw()) }
    }

    /// Get a pointer to the per-lcore default mempool cache.
    pub fn default_cache(&self) -> Option<Cache> {
        lcore::current().and_then(|lcore_id| {
            NonNull::new(unsafe { ffi::rte_mempool_default_cache(self.as_raw(), *lcore_id) }).map(Cache)
        })
    }

    /// Put several objects back in the mempool.
    pub fn generic_put<T: Pooled<R>, R>(&mut self, objs: &[T], cache: Option<Cache>) {
        unsafe {
            ffi::rte_mempool_generic_put(
                self.as_raw(),
                objs.as_ptr() as *const _,
                objs.len() as u32,
                cache.map(|cache| cache.into_raw()).unwrap_or(ptr::null_mut()),
            )
        }
    }

    /// Put several objects back in the mempool.
    ///
    /// This function calls the multi-producer or the single-producer
    /// version depending on the default behavior that was specified at
    /// mempool creation time (see flags).
    pub fn put_bulk<T: Pooled<R>, R>(&mut self, objs: &[T]) {
        unsafe { ffi::rte_mempool_put_bulk(self.as_raw(), objs.as_ptr() as *const _, objs.len() as u32) }
    }

    /// Put several objects back in the mempool.
    ///
    /// This function calls the multi-producer or the single-producer
    /// version depending on the default behavior that was specified at
    /// mempool creation time (see flags).
    pub fn put<T: Pooled<R>, R>(&mut self, obj: T) {
        unsafe { ffi::rte_mempool_put(self.as_raw(), obj.as_raw() as *mut _) }
    }

    /// Get several objects from the mempool.
    ///
    /// If cache is enabled, objects will be retrieved first from cache,
    /// subsequently from the common pool. Note that it can return -ENOENT when
    /// the local cache and common pool are empty, even if cache from other
    /// lcores are full.
    pub fn generic_get<T: Pooled<R>, R>(&mut self, objs: &mut [T], cache: Option<Cache>) -> Result<()> {
        unsafe {
            ffi::rte_mempool_generic_get(
                self.as_raw(),
                objs.as_mut_ptr() as *mut _,
                objs.len() as u32,
                cache.map(|cache| cache.into_raw()).unwrap_or(ptr::null_mut()),
            )
        }
        .as_result()
        .map(|_| ())
    }

    /// Get several objects from the mempool.
    ///
    /// This function calls the multi-consumers or the single-consumer
    /// version, depending on the default behaviour that was specified at
    /// mempool creation time (see flags).
    ///
    /// If cache is enabled, objects will be retrieved first from cache,
    /// subsequently from the common pool. Note that it can return -ENOENT when
    /// the local cache and common pool are empty, even if cache from other
    /// lcores are full.
    pub fn get_bulk<T: Pooled<R>, R>(&mut self, objs: &mut [T]) -> Result<()> {
        unsafe { ffi::rte_mempool_get_bulk(self.as_raw(), objs.as_mut_ptr() as *mut _, objs.len() as u32) }
            .as_result()
            .map(|_| ())
    }

    /// Get several objects from the mempool.
    ///
    /// This function calls the multi-consumers or the single-consumer
    /// version, depending on the default behaviour that was specified at
    /// mempool creation time (see flags).
    ///
    /// If cache is enabled, objects will be retrieved first from cache,
    /// subsequently from the common pool. Note that it can return -ENOENT when
    /// the local cache and common pool are empty, even if cache from other
    /// lcores are full.
    pub fn get<T: Pooled<R>, R>(&mut self) -> Result<T> {
        let mut obj = ptr::null_mut();

        unsafe { ffi::rte_mempool_get(self.as_raw(), &mut obj) }
            .as_result()
            .map(|_| (obj as *mut T::Raw).into())
    }

    /// Get a contiguous blocks of objects from the mempool.
    ///
    /// If cache is enabled, consider to flush it first, to reuse objects
    /// as soon as possible.
    ///
    /// The application should check that the driver supports the operation
    /// by calling rte_mempool_ops_get_info() and checking that `contig_block_size`
    /// is not zero.
    pub fn get_contig_blocks<T: Pooled<R>, R>(&mut self, objs: &mut [T]) -> Result<()> {
        unsafe { ffi::rte_mempool_get_contig_blocks(self.as_raw(), objs.as_mut_ptr() as *mut _, objs.len() as u32) }
            .as_result()
            .map(|_| ())
    }
}
