//! API for lcore and socket manipulation
//!
use std::cmp::Ordering;
use std::fmt;
use std::mem;
use std::ops::Deref;

use crate::ffi;

// use crate::common::config;
use crate::errors::{rte_error, Result};
use crate::common::memory::SocketId;

pub use ffi::LCORE_ID_ANY;
pub use ffi::RTE_MAX_LCORE;

#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct Id(u32);

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub fn id(id: u32) -> Id {
    Id(id)
}

impl<T: Into<u32>> From<T> for Id {
    fn from(id: T) -> Self {
        Id(id.into())
    }
}

impl Deref for Id {
    type Target = u32;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<Id> for usize {
    fn from(id: Id) -> Self {
        id.0 as usize
    }
}

impl PartialEq<u32> for Id {
    fn eq(&self, other: &u32) -> bool {
        self.0 == *other
    }
}

impl PartialOrd<u32> for Id {
    fn partial_cmp(&self, other: &u32) -> Option<Ordering> {
        if *other == ffi::LCORE_ID_ANY {
            None
        } else {
            Some(self.0.cmp(other))
        }
    }
}

impl Id {
    /// Any lcore.
    pub fn any() -> Id {
        Id(ffi::LCORE_ID_ANY)
    }

    pub fn max() -> Id {
        Id(ffi::RTE_MAX_LCORE)
    }

    // /// Get the ID of the physical socket of the specified lcore
    // pub fn socket_id(self) -> SocketId {
    //     unsafe { ffi::lcore_config[self.0 as usize].socket_id as SocketId }
    // }

    /// Test if an lcore is enabled.
    pub fn is_enabled(self) -> bool {
        unsafe { ffi::rte_eal_lcore_role(self.0) == ffi::rte_lcore_role_t_ROLE_RTE }
    }

    pub fn is_master(self) -> bool {
        unsafe { ffi::compat::rte_get_main_lcore() == self.0 }
    }

    /// Get the next enabled lcore ID.
    pub fn next(self) -> Option<Id> {
        next_id(self.0, false, true).map(Id)
    }

    // /// Return the index of the lcore starting from zero.
    // pub fn index(self) -> usize {
    //     unsafe { ffi::lcore_config[self.0 as usize].core_index as usize }
    // }

    /// Test if the core supplied has a specific role
    pub fn has_role(self, role: Role) -> bool {
        unsafe { ffi::rte_lcore_has_role(self.0, role as u32) == 0 }
    }

    /// Get a lcore's role.
    pub fn role(self) -> Role {
        unsafe { Role::from(ffi::rte_eal_lcore_role(self.0)) }
    }
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, FromPrimitive, ToPrimitive)]
pub enum Role {
    Rte = ffi::rte_lcore_role_t_ROLE_RTE,
    Off = ffi::rte_lcore_role_t_ROLE_OFF,
    Service = ffi::rte_lcore_role_t_ROLE_SERVICE,
}

impl From<u32> for Role {
    fn from(role: u32) -> Self {
        unsafe { mem::transmute(role) }
    }
}

/// Return the ID of the execution unit we are running on.
pub fn current() -> Option<Id> {
    match unsafe { ffi::rte_lcore_id() } {
        ffi::LCORE_ID_ANY => None,
        id => Some(id.into()),
    }
}

/// All the enabled lcores.
pub fn enabled() -> Vec<Id> {
    foreach_lcores(false).collect()
}

/// Get the id of the master lcore
pub fn master() -> Id {
    unsafe { id(ffi::compat::rte_get_main_lcore()) }
}

/// Return the number of execution units (lcores) on the system.
pub fn count() -> usize {
    unsafe { ffi::rte_lcore_count() as usize }
}

// /// Return the index of the lcore starting from zero.
// pub fn index(lcore_id: u32) -> Option<usize> {
//     let id = if lcore_id == ffi::LCORE_ID_ANY {
//         current().map(|id| id.0)
//     } else if lcore_id < ffi::RTE_MAX_LCORE {
//         Some(lcore_id)
//     } else {
//         None
//     };

//     id.map(|id| unsafe { ffi::lcore_config[id as usize].core_index as usize })
// }

/// Get the next enabled lcore ID.
pub fn next_id(lcore_id: u32, skip_master: bool, wrap: bool) -> Option<u32> {
    let mut next_id = lcore_id;

    loop {
        next_id += 1;

        if wrap {
            next_id %= ffi::RTE_MAX_LCORE;
        } else if next_id >= ffi::RTE_MAX_LCORE || next_id == lcore_id {
            return None;
        }

        if !Id(next_id).is_enabled() {
            continue;
        }

        if skip_master && Id(next_id).is_master() {
            continue;
        }

        break;
    }

    Some(next_id)
}

/// Return the ID of the physical socket of the logical core we are running on.
pub fn socket_id() -> u32 {
    unsafe { ffi::rte_socket_id() }
}

/// Return number of physical sockets detected on the system.
///
/// Note that number of nodes may not be correspondent to their physical id's:
/// for example, a system may report two socket id's, but the actual socket id's
/// may be 0 and 8.
pub fn socket_count() -> u32 {
    unsafe { ffi::rte_socket_count() }
}

/// Return socket id with a particular index.
///
/// This will return socket id at a particular position in list of all detected
/// physical socket id's. For example, on a machine with sockets [0, 8], passing
/// 1 as a parameter will return 8.
pub fn socket_id_by_idx(idx: u32) -> Result<SocketId> {
    let id = unsafe { ffi::rte_socket_id_by_idx(idx) };

    if id < 0 {
        Err(rte_error())
    } else {
        Ok(id)
    }
}

/// Browse all running lcores.
pub fn foreach<F: FnMut(Id)>(f: F) {
    foreach_lcores(false).for_each(f)
}

/// Browse all running lcores except the master lcore.
pub fn foreach_slave<F: FnMut(Id)>(f: F) {
    foreach_lcores(true).for_each(f)
}

fn foreach_lcores(skip_master: bool) -> impl Iterator<Item = Id> {
    (0..ffi::RTE_MAX_LCORE)
        .map(Id)
        .filter(|lcore_id| lcore_id.is_enabled())
        .filter(move |lcore_id| !skip_master || !lcore_id.is_master())
}
