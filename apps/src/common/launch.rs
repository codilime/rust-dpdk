//! Launch tasks on other lcores
//!
use std::os::raw::{c_int, c_void};

use crate::ffi;
use num_traits::FromPrimitive;

use crate::errors::{AsResult, Result};
use crate::common::lcore;

/// State of an lcore.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, FromPrimitive, ToPrimitive)]
pub enum State {
    Wait = ffi::rte_lcore_state_t_WAIT,
    Running = ffi::rte_lcore_state_t_RUNNING,
    Finished = ffi::rte_lcore_state_t_FINISHED,
}

impl From<ffi::rte_lcore_state_t> for State {
    fn from(s: ffi::rte_lcore_state_t) -> Self {
        State::from_u32(s).unwrap()
    }
}

// Definition of a remote launch function.
pub type LcoreFunc<T> = fn(Option<T>) -> i32;

struct LcoreContext<T> {
    callback: LcoreFunc<T>,
    arg: Option<T>,
}

unsafe extern "C" fn lcore_stub<T>(arg: *mut c_void) -> c_int {
    let ctxt = Box::from_raw(arg as *mut LcoreContext<T>);

    (ctxt.callback)(ctxt.arg)
}

/// Launch a function on another lcore.
///
/// To be executed on the MASTER lcore only.
pub fn remote_launch<T>(callback: LcoreFunc<T>, arg: Option<T>, slave_id: lcore::Id) -> Result<()> {
    let ctxt = Box::into_raw(Box::new(LcoreContext::<T> { callback, arg })) as *mut c_void;

    unsafe { ffi::rte_eal_remote_launch(Some(lcore_stub::<T>), ctxt, *slave_id) }
        .as_result()
        .map(|_| ())
}

/// Launch a function on all lcores.
pub fn mp_remote_launch<T>(callback: LcoreFunc<T>, arg: Option<T>, skip_master: bool) -> Result<()> {
    let ctxt = Box::into_raw(Box::new(LcoreContext::<T> { callback, arg })) as *mut c_void;

    let call_master = if skip_master {
        ffi::compat::rte_rmt_call_main_t_SKIP_MAIN
    } else {
        ffi::compat::rte_rmt_call_main_t_CALL_MAIN
    };

    unsafe { ffi::rte_eal_mp_remote_launch(Some(lcore_stub::<T>), ctxt, call_master) }
        .as_result()
        .map(|_| ())
}

impl lcore::Id {
    /// Get the state of the lcore identified by lcore_id.
    pub fn state(self) -> State {
        unsafe { ffi::rte_eal_get_lcore_state(*self) }.into()
    }

    /// Wait until an lcore finishes its job.
    ///
    /// To be executed on the MASTER lcore only.
    ///
    /// If the slave lcore identified by the slave_id is in a FINISHED state,
    /// switch to the WAIT state. If the lcore is in RUNNING state, wait until
    /// the lcore finishes its job and moves to the FINISHED state.
    ///
    pub fn wait(self) -> JobState {
        let s = unsafe { ffi::rte_eal_wait_lcore(*self) };

        if s == 0 {
            JobState::Wait
        } else {
            JobState::Finished(s)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum JobState {
    Wait,
    Finished(i32),
}

/// Wait until all lcores finish their jobs.
///
/// To be executed on the MASTER lcore only.
/// Issue an rte_eal_wait_lcore() for every lcore.
/// The return values are ignored.
pub fn mp_wait_lcore() {
    unsafe { ffi::rte_eal_mp_wait_lcore() }
}
