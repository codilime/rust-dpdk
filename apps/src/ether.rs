use std::error;
use std::fmt;
use std::mem;
use std::ops::{Deref, DerefMut};
use std::ptr;
use std::result;
use std::str;

use rand::{thread_rng, Rng};

use crate::ffi;

use crate::errors::Result;
use crate::mbuf;
use crate::utils::AsRaw;

// pub use ffi::{
//     ETHER_TYPE_IPv4, ETHER_TYPE_IPv6, ETHER_ADDR_FMT_SIZE, ETHER_CRC_LEN, ETHER_GROUP_ADDR, ETHER_HDR_LEN,
//     ETHER_LOCAL_ADMIN_ADDR, ETHER_MAX_JUMBO_FRAME_LEN, ETHER_MAX_LEN, ETHER_MAX_VLAN_FRAME_LEN, ETHER_MAX_VLAN_ID,
//     ETHER_MIN_LEN, ETHER_MIN_MTU, ETHER_MTU, ETHER_TYPE_1588, ETHER_TYPE_ARP, ETHER_TYPE_ETAG, ETHER_TYPE_LEN,
//     ETHER_TYPE_LLDP, ETHER_TYPE_MPLS, ETHER_TYPE_MPLSM, ETHER_TYPE_QINQ, ETHER_TYPE_RARP, ETHER_TYPE_SLOW,
//     ETHER_TYPE_TEB, ETHER_TYPE_VLAN,
// };

pub const RTE_ETHER_ADDR_LEN: usize = ffi::RTE_ETHER_ADDR_LEN as usize;

#[derive(Debug, Clone, PartialEq)]
pub struct AddrParseError(());

impl fmt::Display for AddrParseError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.write_str(error::Error::description(self))
    }
}

impl error::Error for AddrParseError {
    fn description(&self) -> &str {
        "invalid MAC address syntax"
    }
}

pub type RawEtherAddr = ffi::rte_ether_addr;

/// A 48-bit (6 byte) buffer containing the MAC address
#[derive(Debug, Copy, Clone, Default, PartialEq, Eq, Hash)]
pub struct EtherAddr([u8; RTE_ETHER_ADDR_LEN]);

impl Deref for EtherAddr {
    type Target = [u8; RTE_ETHER_ADDR_LEN];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for EtherAddr {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl EtherAddr {
    /// Creates a new MAC address from six eight-bit octets.
    ///
    /// The result will represent the MAC address a:b:c:d:e:f.
    #[inline]
    pub fn new(a: u8, b: u8, c: u8, d: u8, e: u8, f: u8) -> EtherAddr {
        EtherAddr([a, b, c, d, e, f])
    }

    /// Fast copy an Ethernet address.
    #[inline]
    pub fn copy(from: &[u8; RTE_ETHER_ADDR_LEN], to: &mut [u8; RTE_ETHER_ADDR_LEN]) {
        unsafe { ptr::copy_nonoverlapping(from.as_ptr(), to.as_mut_ptr(), RTE_ETHER_ADDR_LEN) }
    }

    /// Returns the six eight-bit integers that make up this address.
    #[inline]
    pub fn octets(&self) -> &[u8; RTE_ETHER_ADDR_LEN] {
        &self.0
    }

    pub fn into_bytes(self) -> [u8; RTE_ETHER_ADDR_LEN] {
        self.0
    }

    pub fn from_bytes(b: &[u8]) -> result::Result<Self, AddrParseError> {
        if b.len() != RTE_ETHER_ADDR_LEN {
            return Err(AddrParseError(()));
        }

        let mut addr = [0; RTE_ETHER_ADDR_LEN];

        unsafe {
            ptr::copy(b.as_ptr(), addr.as_mut().as_mut_ptr(), b.len());
        }

        Ok(EtherAddr(addr))
    }

    pub fn zeroed() -> Self {
        unsafe { mem::zeroed() }
    }

    pub fn broadcast() -> Self {
        EtherAddr([0xffu8; RTE_ETHER_ADDR_LEN])
    }

    /// Generate a random Ethernet address that is locally administered and not multicast.
    pub fn random() -> Self {
        let mut addr = [0u8; RTE_ETHER_ADDR_LEN];

        thread_rng().fill(&mut addr);

        addr[0] &= !ffi::RTE_ETHER_GROUP_ADDR as u8; // clear multicast bit
        addr[0] |= ffi::RTE_ETHER_LOCAL_ADMIN_ADDR as u8; // set local assignment bit

        EtherAddr(addr)
    }

    /// Check if an Ethernet address is filled with zeros.
    #[inline]
    pub fn is_zero(&self) -> bool {
        self.0 == Self::zeroed().0
    }

    /// Check if an Ethernet address is a unicast address.
    #[inline]
    pub fn is_unicast(&self) -> bool {
        (self.0[0] & ffi::RTE_ETHER_GROUP_ADDR as u8) == 0
    }

    /// Check if an Ethernet address is a multicast address.
    #[inline]
    pub fn is_multicast(&self) -> bool {
        (self.0[0] & ffi::RTE_ETHER_GROUP_ADDR as u8) != 0
    }

    /// Check if an Ethernet address is a broadcast address.
    #[inline]
    pub fn is_broadcast(&self) -> bool {
        self.0 == Self::broadcast().0
    }

    /// Check if an Ethernet address is a universally assigned address.
    #[inline]
    pub fn is_universal(&self) -> bool {
        (self.0[0] & ffi::RTE_ETHER_LOCAL_ADMIN_ADDR as u8) == 0
    }

    ///  Check if an Ethernet address is a locally assigned address.
    #[inline]
    pub fn is_local_admin(&self) -> bool {
        (self.0[0] & ffi::RTE_ETHER_LOCAL_ADMIN_ADDR as u8) != 0
    }

    /// Check if an Ethernet address is a valid address.
    ///
    /// Checks that the address is a unicast address and is not filled with zeros.
    #[inline]
    pub fn is_valid(&self) -> bool {
        self.is_unicast() && !self.is_zero()
    }
}

impl fmt::Display for EtherAddr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            self.0[0], self.0[1], self.0[2], self.0[3], self.0[4], self.0[5]
        )
    }
}

impl From<[u8; 6]> for EtherAddr {
    fn from(addr: [u8; 6]) -> EtherAddr {
        EtherAddr(addr)
    }
}

impl From<*const u8> for EtherAddr {
    fn from(p: *const u8) -> EtherAddr {
        let mut mac = [0u8; RTE_ETHER_ADDR_LEN];

        unsafe {
            ptr::copy_nonoverlapping(p, (&mut mac[..]).as_mut_ptr(), RTE_ETHER_ADDR_LEN);
        }

        EtherAddr(mac)
    }
}

impl From<*mut u8> for EtherAddr {
    fn from(p: *mut u8) -> EtherAddr {
        let mut mac = [0u8; RTE_ETHER_ADDR_LEN];

        unsafe {
            ptr::copy_nonoverlapping(p, (&mut mac[..]).as_mut_ptr(), RTE_ETHER_ADDR_LEN);
        }

        EtherAddr(mac)
    }
}

impl From<RawEtherAddr> for EtherAddr {
    fn from(addr: RawEtherAddr) -> EtherAddr {
        EtherAddr(addr.addr_bytes)
    }
}

impl str::FromStr for EtherAddr {
    type Err = AddrParseError;

    fn from_str(s: &str) -> result::Result<Self, Self::Err> {
        let addr: Vec<u8> = s
            .split(':')
            .filter_map(|part| u8::from_str_radix(part, 16).ok())
            .collect();

        EtherAddr::from_bytes(addr.as_slice())
    }
}

// Ethernet frame types

/// IPv4 Protocol.
pub const ETHER_TYPE_IPV4_BE: u16 = rte_cpu_to_be_16!(ffi::RTE_ETHER_TYPE_IPV4 as u16);
/// IPv6 Protocol.
pub const ETHER_TYPE_IPV6_BE: u16 = rte_cpu_to_be_16!(ffi::RTE_ETHER_TYPE_IPV6 as u16);
/// Arp Protocol.
pub const ETHER_TYPE_ARP_BE: u16 = rte_cpu_to_be_16!(ffi::RTE_ETHER_TYPE_ARP as u16);
/// Reverse Arp Protocol.
pub const ETHER_TYPE_RARP_BE: u16 = rte_cpu_to_be_16!(ffi::RTE_ETHER_TYPE_RARP as u16);
/// IEEE 802.1Q VLAN tagging.
pub const ETHER_TYPE_VLAN_BE: u16 = rte_cpu_to_be_16!(ffi::RTE_ETHER_TYPE_VLAN as u16);
/// IEEE 802.1AS 1588 Precise Time Protocol.
pub const ETHER_TYPE_1588_BE: u16 = rte_cpu_to_be_16!(ffi::RTE_ETHER_TYPE_1588 as u16);
/// Slow protocols (LACP and Marker).
pub const ETHER_TYPE_SLOW_BE: u16 = rte_cpu_to_be_16!(ffi::RTE_ETHER_TYPE_SLOW as u16);
/// Transparent Ethernet Bridging.
pub const ETHER_TYPE_TEB_BE: u16 = rte_cpu_to_be_16!(ffi::RTE_ETHER_TYPE_TEB as u16);

/// Ethernet header: Contains the destination address, source address and frame type.
pub type EtherHdr = ffi::rte_ether_hdr;

/// Ethernet VLAN Header.
pub type VlanHdr = ffi::rte_vlan_hdr;

/// VXLAN protocol header.
pub type VxlanHdr = ffi::rte_vxlan_hdr;

pub trait VlanExt {
    /// Extract VLAN tag information into mbuf
    fn vlan_strip(&mut self) -> Result<()>;
}

impl VlanExt for mbuf::MBuf {
    fn vlan_strip(&mut self) -> Result<()> {
        rte_check!(unsafe { ffi::rte_vlan_strip(self.as_raw()) })
    }
}

/// Insert VLAN tag into mbuf.
pub fn vlan_insert(m: &mut mbuf::RawMBufPtr) -> Result<()> {
    rte_check!(unsafe { ffi::rte_vlan_insert(m) })
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn test_macaddr() {
        let addr = EtherAddr::new(0x18, 0x2b, 0x3c, 0x4d, 0x5e, 0x6f);

        assert_eq!(addr.octets(), &[0x18, 0x2b, 0x3c, 0x4d, 0x5e, 0x6f]);
        assert_eq!(addr.to_string(), "18:2b:3c:4d:5e:6f");

        assert_eq!(addr, EtherAddr::from([0x18, 0x2b, 0x3c, 0x4d, 0x5e, 0x6f]));
        assert_eq!(addr, EtherAddr::from_str("18:2b:3c:4d:5e:6f").unwrap());

        assert!(!addr.is_zero());
        assert!(EtherAddr::zeroed().is_zero());

        assert!(addr.is_unicast());

        let local_addr = EtherAddr::new(0x13, 0x2b, 0x3c, 0x4d, 0x5e, 0x6f);

        assert!(!addr.is_multicast());
        assert!(local_addr.is_multicast());

        assert!(!addr.is_broadcast());
        assert!(EtherAddr::broadcast().is_broadcast());

        assert!(addr.is_universal());
        assert!(!local_addr.is_universal());

        assert!(!addr.is_local_admin());
        assert!(local_addr.is_local_admin());

        let rand_addr = EtherAddr::random();

        assert!(rand_addr.is_unicast());
        assert!(!rand_addr.is_multicast());
        assert!(!rand_addr.is_broadcast());
        assert!(!rand_addr.is_universal());
        assert!(rand_addr.is_local_admin());
        assert!(rand_addr.is_valid());
    }
}
