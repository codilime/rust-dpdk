//! Wrapper for DPDK's environment abstraction layer (EAL).
use crate::ffi;
use arrayvec::*;
use crossbeam::thread::{Scope, ScopedJoinHandle};
use log::{info, warn};
use std::convert::{TryFrom, TryInto};
use std::ffi::CString;
use std::fmt;
use std::marker::PhantomData;
use std::mem::{size_of, MaybeUninit};
use std::ptr::{self, NonNull};
use std::slice;
use std::sync::{Arc, Mutex};
use thiserror::Error;

const MAGIC: &str = "be0dd4ab";

pub const DEFAULT_TX_DESC: u16 = 128;
pub const DEFAULT_RX_DESC: u16 = 128;
pub const DEFAULT_RX_POOL_SIZE: usize = 1023;
pub const DEFAULT_RX_PER_CORE_CACHE: usize = 0;
pub const DEFAULT_PACKET_DATA_LENGTH: usize = 2048;
pub const DEFAULT_PROMISC: bool = true;
pub const DEFAULT_RX_BURST: usize = 32;
pub const DEFAULT_TX_BURST: usize = 32;

/// A garbage collection request.
trait Garbage {
    /// Try to do garbage collection for a certain resource.
    /// Returns true if it succeeded to free an object.
    ///
    /// # Safety
    /// `try_collect` must not be called after it returned `true`.
    unsafe fn try_collect(&mut self) -> bool;
}

/// Shared mutating states that all `Eal` instances share.
struct EalGlobalInner {
    // Whether `setup` has been successfully invoked.
    setup_initialized: bool,
    // List of garbage collection requrests.
    // Each req tries garbage collection and returns true on success.
    // (e.g. `try_free`).
    // TODO: periodically do cleanup.
    garbages: Vec<Box<dyn Garbage>>,
} // TODO Remove this if unnecessary

impl fmt::Debug for EalGlobalInner {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EalGlobalInner")
            .field("setup_initialized", &self.setup_initialized)
            .field("garbages (count)", &self.garbages.len())
            .finish()
    }
}

// Safety: rte_mempool is thread-safe.
unsafe impl Send for EalGlobalInner {}
unsafe impl Sync for EalGlobalInner {}

impl Default for EalGlobalInner {
    #[inline]
    fn default() -> Self {
        Self {
            setup_initialized: false,
            garbages: Default::default(),
        }
    }
}

#[derive(Debug)]
struct EalInner {
    shared: Mutex<EalGlobalInner>,
}

/// DPDK's environment abstraction layer (EAL).
///
/// This object indicates that EAL has been initialized and its APIs are available now.
#[derive(Debug, Clone)]
pub struct Eal {
    inner: Arc<EalInner>,
}

/// How to create NIC queues for a CPU.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum Affinity {
    /// All NICs create queues for the CPU.
    Full,
    /// NICs on the same NUMA node create queues for the CPU.
    Numa,
}

/// Abstract type for DPDK port
#[derive(Debug, Clone)]
pub struct Port {
    inner: Arc<PortInner>,
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct LCoreId(u32);

impl Into<u32> for LCoreId {
    #[inline]
    fn into(self) -> u32 {
        self.0
    }
}

impl LCoreId {
    #[inline]
    fn new(id: u32) -> Self {
        Self(id)
    }

    /// Launch a thread pined to this core (scoped).
    pub fn launch<'s, 'e, F, T>(self, s: &'s Scope<'e>, f: F) -> ScopedJoinHandle<'s, T>
    where
        F: FnOnce() -> T,
        F: Send + 'e,
        T: Send + 'e,
    {
        let lcore_id = self.0;
        s.spawn(move |_| {
            // Safety: foreign function.
            let ret = unsafe {
                dpdk_sys::rte_thread_set_affinity(&mut dpdk_sys::rte_lcore_cpuset(lcore_id))
            };
            if ret < 0 {
                warn!("Failed to set affinity on lcore {}", lcore_id);
            }
            f()
        })
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct SocketId(u32);

impl Into<u32> for SocketId {
    #[inline]
    fn into(self) -> u32 {
        self.0
    }
}

impl SocketId {
    #[inline]
    fn new(id: u32) -> Self {
        Self(id)
    }
}

#[derive(Debug, Error, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorCode {
    #[error("Unknown error code: {}", code)]
    Unknown { code: u8 },
}

impl From<u8> for ErrorCode {
    #[inline]
    fn from(code: u8) -> Self {
        Self::Unknown { code }
    }
}
impl TryFrom<u32> for ErrorCode {
    type Error = <u8 as TryFrom<u32>>::Error;
    #[inline]
    fn try_from(code: u32) -> Result<Self, Self::Error> {
        Ok(Self::Unknown {
            code: code.try_into()?,
        })
    }
}
impl TryFrom<i32> for ErrorCode {
    type Error = <u8 as TryFrom<i32>>::Error;
    #[inline]
    fn try_from(code: i32) -> Result<Self, Self::Error> {
        Ok(Self::Unknown {
            code: (-code).try_into()?,
        })
    }
}

impl Port {
    /// Returns current port index.
    #[inline]
    pub fn port_id(&self) -> u16 {
        self.inner.port_id
    }

    /// Returns NUMA node of current port.
    #[inline]
    pub fn socket_id(&self) -> SocketId {
        SocketId::new(unsafe {
            dpdk_sys::rte_eth_dev_socket_id(self.inner.port_id)
                .try_into()
                .unwrap()
        })
    }

    /// Returns NUMA node of current port.
    #[inline]
    pub fn mac_addr(&self) -> [u8; 6] {
        unsafe {
            let mut mac_addr = MaybeUninit::uninit();
            let ret = dpdk_sys::rte_eth_macaddr_get(self.inner.port_id, mac_addr.as_mut_ptr());
            assert_eq!(ret, 0);
            mac_addr.assume_init().addr_bytes
        }
    }

    /// Change promiscuous mode.
    #[inline]
    pub fn set_promiscuous(&self, set: bool) {
        unsafe {
            if set {
                dpdk_sys::rte_eth_promiscuous_enable(self.port_id());
            } else {
                dpdk_sys::rte_eth_promiscuous_disable(self.port_id());
            }
        }
    }

    /// Start the device.
    #[inline]
    pub fn start(&self) -> Result<(), ErrorCode> {
        let ret = unsafe { dpdk_sys::rte_eth_dev_start(self.port_id()) };
        if ret < 0 {
            return Err(ret.try_into().unwrap());
        }
        Ok(())
    }

    /// Returns current statistics
    #[inline]
    pub fn get_stat(&self) -> PortStat {
        // Safety: foreign function. Uninitialized data structure will be filled.
        let dpdk_stat = unsafe {
            let mut temp = MaybeUninit::uninit();
            let ret = dpdk_sys::rte_eth_stats_get(self.inner.port_id, temp.as_mut_ptr());
            assert_eq!(ret, 0);
            temp.assume_init()
        };
        if self.inner.has_stats_reset {
            PortStat {
                ipackets: dpdk_stat.ipackets,
                opackets: dpdk_stat.opackets,
                ibytes: dpdk_stat.ibytes,
                obytes: dpdk_stat.obytes,
                ierrors: dpdk_stat.ierrors,
                oerrors: dpdk_stat.oerrors,
                imissed: dpdk_stat.imissed,
                rx_nombuf: dpdk_stat.rx_nombuf,
                q_ipackets: dpdk_stat.q_ipackets,
                q_opackets: dpdk_stat.q_opackets,
                q_ibytes: dpdk_stat.q_ibytes,
                q_obytes: dpdk_stat.q_obytes,
                q_errors: dpdk_stat.q_errors,
            }
        } else {
            let prev_stat = self.inner.prev_stat.lock().unwrap();
            fn subtract_array(x: [u64; 16], y: [u64; 16]) -> [u64; 16] {
                let subtract_vals = x.iter().zip(y.iter()).map(|(x, y)| x - y);
                let mut temp: [u64; 16] = Default::default();
                for (ret, val) in (&mut temp).iter_mut().zip(subtract_vals) {
                    *ret = val;
                }
                temp
            }
            PortStat {
                ipackets: dpdk_stat.ipackets - prev_stat.ipackets,
                opackets: dpdk_stat.opackets - prev_stat.opackets,
                ibytes: dpdk_stat.ibytes - prev_stat.ibytes,
                obytes: dpdk_stat.obytes - prev_stat.obytes,
                ierrors: dpdk_stat.ierrors - prev_stat.ierrors,
                oerrors: dpdk_stat.oerrors - prev_stat.oerrors,

                imissed: dpdk_stat.imissed - prev_stat.imissed,
                rx_nombuf: dpdk_stat.rx_nombuf - prev_stat.rx_nombuf,
                q_ipackets: subtract_array(dpdk_stat.q_ipackets, prev_stat.q_ipackets),
                q_opackets: subtract_array(dpdk_stat.q_opackets, prev_stat.q_opackets),
                q_ibytes: subtract_array(dpdk_stat.q_ibytes, prev_stat.q_ibytes),
                q_obytes: subtract_array(dpdk_stat.q_obytes, prev_stat.q_obytes),
                q_errors: subtract_array(dpdk_stat.q_errors, prev_stat.q_errors),
            }
        }
    }

    /// Returns current statistics
    #[inline]
    pub fn reset_stat(&self) {
        // Safety: foreign function.
        if self.inner.has_stats_reset {
            let ret = unsafe { dpdk_sys::rte_eth_stats_reset(self.inner.port_id) };
            assert_eq!(ret, 0);
        } else {
            // Safety: foreign function. Uninitialized data structure will be filled.
            let dpdk_stat = unsafe {
                let mut temp = MaybeUninit::uninit();
                let ret = dpdk_sys::rte_eth_stats_get(self.inner.port_id, temp.as_mut_ptr());
                assert_eq!(ret, 0);
                temp.assume_init()
            };
            let mut prev_stat = self.inner.prev_stat.lock().unwrap();
            prev_stat.ipackets = dpdk_stat.ipackets;
            prev_stat.opackets = dpdk_stat.opackets;
            prev_stat.ibytes = dpdk_stat.ibytes;
            prev_stat.obytes = dpdk_stat.obytes;
            prev_stat.ierrors = dpdk_stat.ierrors;
            prev_stat.oerrors = dpdk_stat.oerrors;
        }
    }

    /// Get link status
    /// Note: this function might block up to 9 seconds.
    /// https://doc.dpdk.org/api/rte__ethdev_8h.html#a56200b0c25f3ecab5abe9bd2b647c215
    #[inline]
    fn get_link(&self) -> LinkStatus {
        // Safety: foreign function.
        unsafe {
            let mut temp = MaybeUninit::uninit();
            let ret = dpdk_sys::rte_eth_link_get(self.inner.port_id, temp.as_mut_ptr());
            assert_eq!(ret, 0);
            temp.assume_init()
        }
    }

    /// Returns true if link is up (connected), false if down.
    #[inline]
    pub fn is_link_up(&self) -> bool {
        self.get_link().link_status() == dpdk_sys::ETH_LINK_UP as u16
    }
}

use dpdk_sys::rte_eth_link as LinkStatus;
pub use dpdk_sys::rte_eth_stats as PortStat;

#[derive(Debug)]
struct PortInner {
    port_id: u16,
    owner_id: u64,
    has_stats_reset: bool,
    prev_stat: Mutex<PortStat>,
    eal: Eal,
}

impl Drop for PortInner {
    #[inline]
    fn drop(&mut self) {
        // Safety: foreign function.
        let ret = unsafe { dpdk_sys::rte_eth_dev_owner_unset(self.port_id, self.owner_id) };
        assert_eq!(ret, 0);
        unsafe {
            dpdk_sys::rte_eth_dev_stop(self.port_id);
            dpdk_sys::rte_eth_dev_close(self.port_id);
        }
        // TODO following code causes segmentation fault.  Its DPDK's bug that
        // `rte_eth_dev_owner_delete` does not check whether `rte_eth_devices[port_id].data` is
        // null.  Safety: foreign function.
        // let ret = unsafe { dpdk_sys::rte_eth_dev_owner_delete(self.owner_id) };
        // assert_eq!(ret, 0);
        info!("Port {} cleaned up", self.port_id);
    }
}

/// Abstract type for DPDK uninitialized port
#[derive(Debug)]
pub struct UninitPort {
    port_id: u16,
    eal: Eal,
}

pub struct RteEthConf {
    pub data: dpdk_sys::rte_eth_conf,
}

impl RteEthConf {
    pub fn new() -> RteEthConf {
        RteEthConf {
            data: unsafe { std::mem::zeroed() },
        }
    }
}

impl UninitPort {
    /// Initialize port. Configure specified number of rx and tx queues.
    pub fn init<MPoolPriv: Zeroable>(
        self,
        rx_queue_count: u16,
        tx_queue_count: u16,
        opt_port_conf: Option<RteEthConf>,
    ) -> (Port, (Vec<RxQ<MPoolPriv>>, Vec<TxQ<'static>>)) {
        let mut dev_info: dpdk_sys::rte_eth_dev_info = unsafe { std::mem::zeroed() };
        // Safety: foreign function.
        unsafe { dpdk_sys::rte_eth_dev_info_get(self.port_id, &mut dev_info) };

        // TODO: return result istead of assert
        assert!(dev_info.max_rx_queues >= rx_queue_count);
        assert!(dev_info.max_tx_queues >= tx_queue_count);

        let mut owner_id = 0;
        // Safety: foreign function.
        let ret = unsafe { dpdk_sys::rte_eth_dev_owner_new(&mut owner_id) };
        assert_eq!(ret, 0);

        let mut owner = dpdk_sys::rte_eth_dev_owner {
            id: owner_id,
            // Safety: `c_char` array can accept zeroed data.
            name: unsafe { MaybeUninit::zeroed().assume_init() },
        };
        let owner_name = format!("rust_dpdk_port_owner_{}", self.port_id);
        let name_cstring = CString::new(owner_name).unwrap();
        let name_bytes = name_cstring.as_bytes_with_nul();
        // Safety: converting &[u8] string into &[i8] string.
        owner.name[0..name_bytes.len()]
            .copy_from_slice(unsafe { &*(name_bytes as *const [u8] as *const [i8]) });
        // Safety: foreign function.
        let ret = unsafe { dpdk_sys::rte_eth_dev_owner_set(self.port_id, &owner) };
        assert_eq!(ret, 0);

        let mut port = Port {
            inner: Arc::new(PortInner {
                port_id: self.port_id,
                owner_id,
                has_stats_reset: true,
                // Safety: PortStat allows zeroed structure.
                prev_stat: Mutex::new(unsafe { MaybeUninit::zeroed().assume_init() }),
                eal: self.eal,
            }),
        };

        let port_conf = if let Some(some_port_conf) = opt_port_conf {
            some_port_conf
        } else {
            let mut port_conf = RteEthConf::new();
            port_conf.data.rxmode.max_rx_pkt_len = dpdk_sys::RTE_ETHER_MAX_LEN;
            port_conf.data.rxmode.mq_mode = dpdk_sys::rte_eth_rx_mq_mode_ETH_MQ_RX_NONE;
            port_conf.data.txmode.mq_mode = dpdk_sys::rte_eth_tx_mq_mode_ETH_MQ_TX_NONE;
            if rx_queue_count > 1 {
                // Enable RSS.
                port_conf.data.rxmode.mq_mode = dpdk_sys::rte_eth_rx_mq_mode_ETH_MQ_RX_RSS;
                port_conf.data.rx_adv_conf.rss_conf.rss_hf = (dpdk_sys::ETH_RSS_NONFRAG_IPV4_UDP
                    | dpdk_sys::ETH_RSS_NONFRAG_IPV4_TCP)
                    .into();
                // TODO set symmetric RSS for TCP/IP
            }
            port_conf
        };

        // Safety: foreign function.
        let ret = unsafe {
            dpdk_sys::rte_eth_dev_configure(
                port.inner.port_id,
                rx_queue_count,
                tx_queue_count,
                &port_conf.data,
            )
        };
        assert_eq!(ret, 0);

        let rxq = (0..rx_queue_count)
            .map(|queue_id| {
                let mpool = port.inner.eal.create_mpool(
                    format!("rxq_{}_{}_{}", MAGIC, port.inner.port_id, queue_id),
                    DEFAULT_RX_POOL_SIZE,
                    DEFAULT_RX_PER_CORE_CACHE,
                    DEFAULT_PACKET_DATA_LENGTH,
                    Some(port.socket_id()),
                );
                let ret = unsafe {
                    dpdk_sys::rte_eth_rx_queue_setup(
                        port.inner.port_id,
                        queue_id,
                        DEFAULT_RX_DESC,
                        port.socket_id().into(),
                        &dev_info.default_rxconf,
                        mpool.inner.ptr.as_ptr(),
                    )
                };
                assert_eq!(ret, 0);
                RxQ {
                    queue_id,
                    port: port.clone(),
                    mpool: mpool.inner,
                    _not_threadsafe: PhantomData,
                }
            })
            .collect::<Vec<_>>();

        let txq = (0..tx_queue_count)
            .map(|queue_id| {
                let ret = unsafe {
                    dpdk_sys::rte_eth_tx_queue_setup(
                        port.inner.port_id,
                        queue_id,
                        DEFAULT_RX_DESC,
                        port.socket_id().into(),
                        &dev_info.default_txconf,
                    )
                };
                assert_eq!(ret, 0);
                TxQ {
                    queue_id,
                    port: port.clone(),
                    _pool: PhantomData,
                }
            })
            .collect::<Vec<_>>();

        let ret = unsafe { dpdk_sys::rte_eth_stats_reset(self.port_id) };
        if ret == -(dpdk_sys::ENOTSUP as i32) {
            warn!("stats_reset is not supported. Fallback to software emulation.");
            Arc::get_mut(&mut port.inner).unwrap().has_stats_reset = false;
        }

        (port, (rxq, txq))
    }
}

/// Traits for `zeroable` structures.
///
/// Related issue: https://github.com/rust-lang/rfcs/issues/2626
///
/// DPDK provides customizable per-packet metadata. However, it is initialized via
/// `memset(.., 0, ..)`, and its destructor is not called.
/// A structure must be safe from `MaybeUninit::zeroed().assume_init()`
/// and it must not implement `Drop` trait.
pub unsafe trait Zeroable: Sized {
    fn zeroed() -> Self {
        // Safety: contraints from this trait.
        unsafe { MaybeUninit::zeroed().assume_init() }
    }
}

unsafe impl Zeroable for () {}

/// Abstract type for DPDK MPool
#[derive(Debug, Clone)]
pub struct MPool<MPoolPriv: Zeroable> {
    inner: Arc<MPoolInner<MPoolPriv>>,
}

#[derive(Debug)]
struct MPoolInner<MPoolPriv: Zeroable> {
    ptr: NonNull<dpdk_sys::rte_mempool>,
    eal: Arc<EalInner>,
    _phantom: PhantomData<MPoolPriv>,
}

/// # Safety
/// Mempools are thread-safe.
/// https://doc.dpdk.org/guides/prog_guide/thread_safety_dpdk_functions.html
unsafe impl<MPoolPriv: Zeroable> Send for MPoolInner<MPoolPriv> {}
unsafe impl<MPoolPriv: Zeroable> Sync for MPoolInner<MPoolPriv> {}

impl<MPoolPriv: Zeroable> Drop for MPoolInner<MPoolPriv> {
    #[inline]
    fn drop(&mut self) {
        // Check whether the pool can be destroyed now.
        // Note: I am the only reference to the pool object.
        struct MPoolGcReq {
            ptr: NonNull<dpdk_sys::rte_mempool>,
        }
        impl Garbage for MPoolGcReq {
            #[inline]
            unsafe fn try_collect(&mut self) -> bool {
                if dpdk_sys::rte_mempool_full(self.ptr.as_ptr()) > 0 {
                    dpdk_sys::rte_mempool_free(self.ptr.as_ptr());
                    true
                } else {
                    false
                }
            }
        }
        let mut ret = MPoolGcReq { ptr: self.ptr };
        if !unsafe { ret.try_collect() } {
            // Case: with dangling mbufs
            // Note: deferred free via Eal
            self.eal.shared.lock().unwrap().garbages.push(Box::new(ret));
        }
    }
}

impl<MPoolPriv: Zeroable> MPool<MPoolPriv> {
    /// Allocate a `Packet` from the pool.
    #[inline]
    pub fn alloc(&self) -> Option<Packet<'_, MPoolPriv>> {
        // Safety: foreign function.
        // `alloc` is temporarily unsafe. Leaving this unsafe block.
        let pkt_ptr = unsafe { dpdk_sys::rte_pktmbuf_alloc(self.inner.ptr.as_ptr()) };

        Some(Packet {
            ptr: NonNull::new(pkt_ptr)?,
            _phantom: PhantomData {},
            _pool: PhantomData {},
        })
    }

    /// Allocate packets and fill them in the remaining capacity of the given `ArrayVec`.
    #[inline]
    pub fn alloc_bulk<'pool, A: Array<Item = Packet<'pool, MPoolPriv>>>(
        &'pool self,
        buffer: &mut ArrayVec<A>,
    ) -> bool {
        let current_offset = buffer.len();
        let capacity = buffer.capacity();
        let remaining = capacity - current_offset;
        // Safety: foreign function.
        // Safety: manual arrayvec manipulation.
        // `alloc_bulk` is temporarily unsafe. Leaving this unsafe block.
        unsafe {
            let pkt_buffer = buffer.as_mut_ptr() as *mut *mut dpdk_sys::rte_mbuf;
            let ret = dpdk_sys::rte_pktmbuf_alloc_bulk(
                self.inner.ptr.as_ptr(),
                pkt_buffer.add(current_offset),
                remaining as u32,
            );

            if ret == 0 {
                buffer.set_len(capacity);
                return true;
            }
        }
        false
    }
}

/// An owned reference to `Packet`.
///
/// Equivalent to Mbuf
#[derive(Debug)]
#[repr(transparent)]
pub struct Packet<'pool, MPoolPriv: Zeroable> {
    ptr: NonNull<dpdk_sys::rte_mbuf>,
    _phantom: PhantomData<MPoolPriv>,
    _pool: PhantomData<&'pool MPool<MPoolPriv>>,
}

unsafe impl<MPoolPriv: Zeroable> Send for Packet<'_, MPoolPriv> {}
unsafe impl<MPoolPriv: Zeroable> Sync for Packet<'_, MPoolPriv> {}

impl<MPoolPriv: Zeroable> Packet<'_, MPoolPriv> {
    /// Returns whether `data_len` is zero.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Read data_len field
    #[inline]
    pub fn len(&self) -> usize {
        unsafe { self.ptr.as_ref().data_len }.into()
    }

    /// Read buf_len field
    #[inline]
    pub fn capacity(&self) -> usize {
        unsafe { self.ptr.as_ref().buf_len }.into()
    }

    /// Read priv_data field
    /// TODO we will save non-public, FPS-specific metadata to `MPoolPriv`.
    #[inline]
    pub fn priv_data(&self) -> &MPoolPriv {
        // Safety: All MPool instances have reserved private data for `MPoolPriv`.
        unsafe { &*(dpdk_sys::rte_mbuf_to_priv(self.ptr.as_ptr()) as *const MPoolPriv) }
    }

    /// Read/Write priv_data field
    /// TODO we will save non-public, FPS-specific metadata to `MPoolPriv`.
    #[inline]
    pub fn priv_data_mut(&mut self) -> &mut MPoolPriv {
        // Safety: All MPool instances have reserved private data for `MPoolPriv`.
        unsafe { &mut *(dpdk_sys::rte_mbuf_to_priv(self.ptr.as_ptr()) as *mut MPoolPriv) }
    }

    /// Retrieve read-only slice of packet buffer (regardless of `data_offset`).
    /// TODO: use `rte_pktmbuf_read` later?
    #[inline]
    pub fn buffer(&self) -> &[u8] {
        unsafe {
            let mbuf_ptr = self.ptr.as_ptr();
            slice::from_raw_parts(
                (*mbuf_ptr)
                    .buf_addr
                    .add((*mbuf_ptr).data_off.try_into().unwrap()) as *const u8,
                (*mbuf_ptr).buf_len.into(),
            )
        }
    }

    /// Retrieve writable slice of packet buffer (regardless of `data_offset`).
    #[inline]
    pub fn buffer_mut(&mut self) -> &mut [u8] {
        unsafe {
            let mbuf_ptr = self.ptr.as_ptr();
            slice::from_raw_parts_mut(
                (*mbuf_ptr)
                    .buf_addr
                    .add((*mbuf_ptr).data_off.try_into().unwrap()) as *mut u8,
                (*mbuf_ptr).buf_len.into(),
            )
        }
    }

    /// Change the packet length
    /// TODO: Do we need this? Shall we replace it with prepend/append?
    #[inline]
    pub fn set_len(&mut self, size: usize) {
        // Safety: buffer boundary is guarded by the assert statement.
        unsafe {
            let mbuf_ptr = self.ptr.as_ptr();
            assert!((*mbuf_ptr).buf_len >= size as u16);
            (*mbuf_ptr).data_len = size as u16;
            (*mbuf_ptr).pkt_len = size as u32;
        }
    }

    /// Retrieve read-only slice of packet's data buffer.
    /// TODO: use `rte_pktmbuf_read` instead?
    #[inline]
    pub fn data(&self) -> &[u8] {
        &self.buffer()[0..self.len()]
    }

    /// Retrieve writable slice of packet's data buffer.
    #[inline]
    pub fn data_mut(&mut self) -> &mut [u8] {
        let len = self.len();
        &mut self.buffer_mut()[0..len]
    }

    /// Skip first n bytes of this packet.
    /// Panic: when size is out of bound.
    #[inline]
    pub fn trim_head(&mut self, size: usize) {
        // Safety: foreign function.
        unsafe {
            let ret = dpdk_sys::rte_pktmbuf_adj(self.ptr.as_ptr(), size as u16);
            assert_ne!(ret, ptr::null_mut());
        }
    }

    /// Skip last n bytes of this packet.
    /// Panic: when size is out of bound.
    #[inline]
    pub fn trim_tail(&mut self, size: usize) {
        // Safety: foreign function.
        unsafe {
            let ret = dpdk_sys::rte_pktmbuf_trim(self.ptr.as_ptr(), size as u16);
            assert_eq!(ret, 0);
        }
    }

    /// Reset headroom.
    /// Note: tail can be reset by setting `data_len` to its buffer capacity.
    #[inline]
    pub fn reset_headroom(&mut self) {
        // Safety: foreign function.
        unsafe {
            dpdk_sys::rte_pktmbuf_reset_headroom(self.ptr.as_ptr());
        }
    }

    /// Prepend packet's data buffer to left.
    /// Panic: when size is out of bound.
    #[inline]
    pub fn prepend(&mut self, size: usize) {
        // Safety: foreign function.
        unsafe {
            let ret = dpdk_sys::rte_pktmbuf_prepend(self.ptr.as_ptr(), size as u16);
            assert_ne!(ret, ptr::null_mut());
        }
    }

    /// Prepend packet's data buffer to right.
    /// Panic: when size is out of bound.
    #[inline]
    pub fn append(&mut self, size: usize) {
        // Safety: foreign function.
        unsafe {
            let ret = dpdk_sys::rte_pktmbuf_append(self.ptr.as_ptr(), size as u16);
            assert_ne!(ret, ptr::null_mut());
        }
    }
}

impl<MPoolPriv: Zeroable> Drop for Packet<'_, MPoolPriv> {
    #[inline]
    fn drop(&mut self) {
        // Safety: foreign function.
        unsafe {
            dpdk_sys::rte_pktmbuf_free(self.ptr.as_ptr());
        }
    }
}

/// Abstract type for DPDK RxQ
///
/// Note: RxQ requires a dedicated mempool to receive incoming packets.
#[derive(Debug)]
pub struct RxQ<MPoolPriv: Zeroable> {
    queue_id: u16,
    port: Port,
    mpool: Arc<MPoolInner<MPoolPriv>>,
    /// !Sync marker. RxQ is supposed to be accessed only from a single thread
    // Note: This single-threaded limitation could also be implemented by making rx() take
    // exclusive reference (`&mut self`), but currently `rx` takes `&self`.
    _not_threadsafe: PhantomData<std::cell::Cell<u8>>,
}

impl<MPoolPriv: Zeroable> Drop for RxQ<MPoolPriv> {
    #[inline]
    fn drop(&mut self) {
        // Safety: foreign function.
        //
        // Note: dynamically starting/stopping queue may not be supported by the driver.
        let ret =
            unsafe { dpdk_sys::rte_eth_dev_rx_queue_stop(self.port.inner.port_id, self.queue_id) };
        if ret != 0 {
            warn!(
                "RxQ::drop, non-severe error code({}) while stopping queue {}:{}",
                ret, self.port.inner.port_id, self.queue_id
            );
        }
    }
}

impl<MPoolPriv: Zeroable> RxQ<MPoolPriv> {
    /// Returns current queue index.
    #[inline]
    pub fn queue_id(&self) -> u16 {
        self.queue_id
    }

    /// Receive packets and store them in the given arrayvec.
    ///
    /// Note: The lifetime of packets is a little bit too constrained, as it's tied to RxQ, but in
    /// principle it should be tied only to RxQ's mempool. This could change in the future, but
    /// seems fine for now.
    // This conceptually needs unique borrow (`&mut self`,) but it takes `&self` reference, because
    // we want Packet to only have shared borrow of self. Because of that RxQ is marked as !Sync
    // (to block calling rx() from multiple threads).
    #[inline]
    pub fn rx<'pool, A: Array<Item = Packet<'pool, MPoolPriv>>>(
        &'pool self,
        buffer: &mut ArrayVec<A>,
    ) {
        let current = buffer.len();
        let remaining = buffer.capacity() - current;
        unsafe {
            let pkt_buffer = buffer.as_mut_ptr() as *mut *mut dpdk_sys::rte_mbuf;
            let cnt = dpdk_sys::rte_eth_rx_burst(
                self.port.inner.port_id,
                self.queue_id,
                pkt_buffer.add(current),
                remaining as u16,
            );
            buffer.set_len(current + cnt as usize);
        }
    }

    /// Get port of this queue.
    #[inline]
    pub fn port(&self) -> &Port {
        &self.port
    }
}

/// Abstract type for DPDK TxQ
///
/// Note: while RxQ requires a dedicated mempool, Tx operation takes `MBuf`s which are allocated by
/// other RxQ's mempool or other externally allocated mempools. Thus, TxQ itself does not require
/// its own mempool.
///
/// Note: The 'pool lifetime parameter ensures that MPool used in [`TxQ::tx()`] outlives the TxQ
#[derive(Debug)]
pub struct TxQ<'pool> {
    queue_id: u16,
    port: Port,
    _pool: PhantomData<&'pool MPool<()>>,
}

impl Drop for TxQ<'_> {
    #[inline]
    fn drop(&mut self) {
        // Safety: foreign function.
        //
        // Note: dynamically starting/stopping queue may not be supported by the driver.
        let ret =
            unsafe { dpdk_sys::rte_eth_dev_tx_queue_stop(self.port.inner.port_id, self.queue_id) };
        if ret != 0 {
            warn!(
                "TxQ::drop, non-severe error code({}) while stopping queue {}:{}",
                ret, self.port.inner.port_id, self.queue_id
            );
        }
    }
}

impl<'pool> TxQ<'pool> {
    /// Returns current queue index.
    #[inline]
    pub fn queue_id(&self) -> u16 {
        self.queue_id
    }

    /// Send a burst of packets on a transmit queue of an Ethernet device
    ///
    /// It's possible that not all packets will be transmitted (when the descriptor limit in
    /// transmit ring is reached). In that case, the `buffer` will contain packets that were _not_
    /// transmitted.
    ///
    /// Note: When the function finishes, the packets are guaranteed to be transmitted to the
    /// `TxQ`'s internal ring, not through physical interface. You can use
    /// [`.port().get_stat()`][`Port::get_stat`] to check port stats.
    // Note: This function would compile also with &self receiver, but we're using &mut to prevent
    // calling tx() from multiple threads.
    #[inline]
    pub fn tx<MPoolPriv: Zeroable + 'pool, A: Array<Item = Packet<'pool, MPoolPriv>>>(
        &mut self,
        buffer: &mut ArrayVec<A>,
    ) {
        let current = buffer.len();
        // Safety: this block is very dangerous.

        // Get raw pointer of arrayvec
        let pkt_buffer = buffer.as_mut_ptr() as *mut *mut dpdk_sys::rte_mbuf;

        // Try transmit packets. It will return number of successfully transmitted packets.
        // Successfully transmitted packets are automatically dropped by `rte_eth_tx_burst`.
        // Safety: foreign function.
        // Safety: `pkt_buffer` is safe to read till `pkt_buffer[current]`.
        let cnt = unsafe {
            dpdk_sys::rte_eth_tx_burst(
                self.port.inner.port_id,
                self.queue_id,
                pkt_buffer,
                current as u16,
            ) as usize
        };

        // Remaining packets are moved to the beginning of the vector.
        let remaining = current - cnt;
        // Safety: pkt_buffer[cur...len] are unsent thus safe to be accessed.
        // This line moves pkts at tail to the head of the array.
        unsafe { ptr::copy(pkt_buffer.add(cnt), pkt_buffer, remaining) };

        // Safety: headers are filled with unsent packets and it is safe to set the length.
        unsafe { buffer.set_len(remaining) };
    }

    /// Make copies of MBufs and transmit them
    ///
    /// Returns number of packets sent (transmitted to the send queue)
    ///
    /// See [`TxQ::tx()`]
    #[inline]
    pub fn tx_cloned<MPoolPriv: Zeroable + 'pool, A: Array<Item = Packet<'pool, MPoolPriv>>>(
        &mut self,
        buffer: &ArrayVec<A>,
    ) -> usize {
        let current = buffer.len();

        for pkt in buffer {
            // Safety: foreign function.
            // Note: It does not cause memory leak as tx_burst decreases the reference count.
            unsafe { dpdk_sys::rte_pktmbuf_refcnt_update(pkt.ptr.as_ptr(), 1) };
        }

        // Get raw pointer of arrayvec
        let pkt_buffer = buffer.as_ptr() as *mut *mut dpdk_sys::rte_mbuf;

        // Try transmit packets. It will return number of successfully transmitted packets.
        // Successfully transmitted packets are automatically dropped by `rte_eth_tx_burst`.
        //
        // Safety: foreign function.
        // Safety: `pkt_buffer` is safe to read till `pkt_buffer[current]`.
        let cnt = unsafe {
            dpdk_sys::rte_eth_tx_burst(
                self.port.inner.port_id,
                self.queue_id,
                pkt_buffer,
                current as u16,
            )
        };
        let cnt = usize::from(cnt);

        // We have to manually free unsent packets, or some packets will leak.
        for i in cnt..current {
            // Safety: foreign function.
            // Safety: pkt's refcount is already increased thus there is no use-after-free.
            unsafe { dpdk_sys::rte_pktmbuf_free(*(pkt_buffer.add(i))) };
        }
        // As all mbuf's references are already increases, we do not have to free the arrayvec.

        cnt
    }

    /// Get port of this queue.
    #[inline]
    pub fn port(&self) -> &Port {
        &self.port
    }
}

impl Eal {
    /// Create an `Eal` instance.
    ///
    /// It takes command-line arguments and consumes used arguments.
    #[inline]
    pub fn new(args: &mut Vec<String>) -> Result<Self, ErrorCode> {
        Ok(Eal {
            inner: Arc::new(EalInner::new(args)?),
        })
    }

    /// Create a new `MPool`.
    ///
    /// # Panic
    /// Pool name must be globally unique, otherwise it will panic.
    ///
    /// @param n The number of elements in the mbuf pool.
    ///
    /// @param cache_size Size of the per-core object cache.
    ///
    /// @param data_room_size Size of data buffer in each mbuf, including RTE_PKTMBUF_HEADROOM.
    ///
    /// @param socket_id The socket identifier where the memory should be allocated. The value can
    /// be `None` (corresponds to DPDK's *SOCKET_ID_ANY*) if there is no NUMA constraint for the
    /// reserved zone.
    #[inline]
    pub fn create_mpool<S: AsRef<str>, MPoolPriv: Zeroable>(
        &self,
        name: S,
        n: usize,
        cache_size: usize,
        data_room_size: usize,
        socket_id: Option<SocketId>,
    ) -> MPool<MPoolPriv> {
        let pool_name = CString::new(name.as_ref()).unwrap();

        // Safety: foreign function.
        let ptr = unsafe {
            dpdk_sys::rte_pktmbuf_pool_create(
                pool_name.into_bytes_with_nul().as_ptr() as *mut i8,
                n.try_into().unwrap(),
                cache_size as u32,
                (((size_of::<MPoolPriv>() + 7) / 8) * 8) as u16,
                data_room_size.try_into().unwrap(),
                socket_id
                    .map(|x| x.0 as i32)
                    .unwrap_or(dpdk_sys::SOCKET_ID_ANY),
            )
        };

        let inner = Arc::new(MPoolInner {
            ptr: NonNull::new(ptr).unwrap(), // will panic if the given name is not unique.
            eal: self.inner.clone(),
            _phantom: PhantomData {},
        });

        // The pointer to the new allocated mempool, on success. NULL on error with rte_errno set appropriately.
        // https://doc.dpdk.org/api/rte__mbuf_8h.html
        MPool { inner }
    }

    /// Get list of available, uninitialized ports.
    /// Should be called once.
    #[inline]
    pub fn ports(&self) -> Result<Vec<UninitPort>, ErrorCode> {
        let mut shared_mut = self.inner.shared.lock().unwrap();
        if shared_mut.setup_initialized {
            // Already initialized.
            return Err(dpdk_sys::EALREADY.try_into().unwrap());
        }
        let port_list = (0..u16::try_from(dpdk_sys::RTE_MAX_ETHPORTS).unwrap())
            .filter(|index| {
                // Safety: foreign function.
                unsafe { dpdk_sys::rte_eth_dev_is_valid_port(*index) > 0 }
            })
            .map(|port_id| UninitPort {
                port_id,
                eal: self.clone(),
            })
            .collect::<Vec<_>>();
        shared_mut.setup_initialized = true;
        Ok(port_list)
    }

    /// Get a vector of enabled lcores.
    #[inline]
    pub fn lcores(&self) -> Vec<LCoreId> {
        (0..dpdk_sys::RTE_MAX_LCORE)
            .filter(|index| unsafe { dpdk_sys::rte_lcore_is_enabled(*index) > 0 })
            .map(|lcore_id| LCoreId::new(lcore_id))
            .collect()
    }

    /// Get a vector of enabled lcores with socket ids.
    #[inline]
    pub fn lcores_sockets(&self) -> Vec<(LCoreId, SocketId)> {
        self.lcores()
            .into_iter()
            .map(|lcore_id| {
                // Safety: foreign function.
                let socket_id = unsafe { dpdk_sys::rte_lcore_to_socket_id(lcore_id.into()) };
                (lcore_id, SocketId::new(socket_id))
            })
            .collect()
    }
}

pub use dpdk_sys::EalStaticFunctions as EalGlobalApi;

unsafe impl EalGlobalApi for Eal {}

impl EalInner {
    // Create `EalInner`.
    #[inline]
    fn new(args: &mut Vec<String>) -> Result<Self, ErrorCode> {
        // To prevent DPDK PMDs' being unlinked, we explicitly create symbolic dependency via
        // calling `load_drivers`.
        dpdk_sys::load_drivers();

        // DPDK returns number of consumed argc
        // Safety: foriegn function (safe unless there is a bug)
        let ret = unsafe { ffi::run_with_args(dpdk_sys::rte_eal_init, &*args) };
        if ret < 0 {
            return Err(ret.try_into().unwrap());
        }

        // Strip first n args and return the remaining
        args.drain(..ret as usize);
        Ok(EalInner {
            shared: Mutex::new(Default::default()),
        })
    }
}

impl Drop for EalInner {
    #[inline]
    fn drop(&mut self) {
        // Safety: foriegn function (safe unless there is a bug)
        unsafe {
            for mut gc_req in self.shared.get_mut().unwrap().garbages.drain(..) {
                let ret = gc_req.try_collect();
                assert_eq!(ret, true);
            }

            let ret = dpdk_sys::rte_eal_cleanup();
            if ret == -(dpdk_sys::ENOTSUP as i32) {
                warn!("EAL cleanup is not implemented.");
                return;
            }
            assert_eq!(ret, 0);
            info!("EAL cleaned up");
        }
    }
}
