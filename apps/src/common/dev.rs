//! RTE PMD Driver Registration Interface
//!
//! This file manages the list of device drivers.
//!
use std::ffi::CStr;
use std::mem;
use std::os::raw::{c_char, c_void};

use crate::errors::{AsResult, Result};
use crate::ffi;
use crate::utils::AsCString;

/// The device event type.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, FromPrimitive, ToPrimitive)]
pub enum Event {
    /// device being added
    Add = ffi::rte_dev_event_type_RTE_DEV_EVENT_ADD,
    /// device being removed
    Remove = ffi::rte_dev_event_type_RTE_DEV_EVENT_REMOVE,
    /// max value of this enum
    Max = ffi::rte_dev_event_type_RTE_DEV_EVENT_MAX,
}

pub type RawDevice = ffi::rte_device;
pub type RawDevicePtr = *mut ffi::rte_device;

#[repr(transparent)]
#[derive(Debug)]
pub struct Device(RawDevicePtr);

impl From<RawDevicePtr> for Device {
    fn from(p: RawDevicePtr) -> Self {
        Device(p)
    }
}

impl Device {
    /// Query status of a device.
    pub fn is_probed(&self) -> bool {
        unsafe { ffi::rte_dev_is_probed(self.0) != 0 }
    }

    /// Remove one device.
    ///
    /// In multi-process, it will request other processes to remove the same device.
    /// A failure, in any process, will rollback the action
    pub fn remove(&self) -> Result<()> {
        unsafe { ffi::rte_dev_remove(self.0) }.as_result().map(|_| ())
    }
}

///  Hotplug add a given device to a specific bus.
///
///  In multi-process, it will request other processes to add the same device.
///  A failure, in any process, will rollback the action
pub fn hotplug_add(busname: &str, devname: &str, drvargs: &str) -> Result<()> {
    let busname = busname.as_cstring();
    let devname = devname.as_cstring();
    let drvargs = drvargs.as_cstring();

    unsafe { ffi::rte_eal_hotplug_add(busname.as_ptr(), devname.as_ptr(), drvargs.as_ptr()) }
        .as_result()
        .map(|_| ())
}

///  Hotplug remove a given device from a specific bus.
///
///  In multi-process, it will request other processes to remove the same device.
///  A failure, in any process, will rollback the action
pub fn hotplug_remove(busname: &str, devname: &str) -> Result<()> {
    let busname = busname.as_cstring();
    let devname = devname.as_cstring();

    unsafe { ffi::rte_eal_hotplug_remove(busname.as_ptr(), devname.as_ptr()) }
        .as_result()
        .map(|_| ())
}

pub type EventCallback<T> = fn(devname: &str, Event, Option<T>);

struct EventContext<T> {
    callback: EventCallback<T>,
    arg: Option<T>,
}

unsafe extern "C" fn event_stub<T>(devname: *const c_char, event: ffi::rte_dev_event_type, arg: *mut c_void) {
    let devname = CStr::from_ptr(devname);
    let ctxt = Box::from_raw(arg as *mut EventContext<T>);

    (ctxt.callback)(devname.to_str().unwrap(), mem::transmute(event), ctxt.arg)
}

///  It registers the callback for the specific device.
///  Multiple callbacks cal be registered at the same time.
pub fn event_callback_register<T>(devname: &str, callback: EventCallback<T>, arg: Option<T>) -> Result<()> {
    let devname = devname.as_cstring();
    let ctxt = Box::into_raw(Box::new(EventContext::<T> { callback, arg }));

    unsafe { ffi::rte_dev_event_callback_register(devname.as_ptr(), Some(event_stub::<T>), ctxt as *mut _) }
        .as_result()
        .map(|_| ())
}
