//! Useful tools for C FFI

use std::ffi::CString;
use std::ops;
use std::os::raw::{c_char, c_int};
use std::ptr;

/// Call a main-like function with an argument vector
///
/// The `func` function is allowed to modify argv and the modifications will be visible in the
/// given vector. `modified_range` should return a range of `argv` that is expected to be modified
/// based on `func`'s return value. If not interested in any modifications, just pass `|_| None`.
///
/// # Safety
///
/// Safety depends on the safety of the target FFI function.
pub unsafe fn run_with_args(
    func: unsafe extern "C" fn(c_int, *mut *mut c_char) -> c_int,
    modified_range: impl FnOnce(c_int) -> Option<ops::RangeFrom<usize>>,
    args: &mut Vec<String>,
) -> i32 {
    // 1. First clone the string values into safe storage.
    let cstring_buffer: Vec<_> = args
        .iter()
        .map(|arg| CString::new(arg.clone()).expect("String to CString conversion failed"))
        .collect();

    // 2. Total number of args is fixed.
    let argc = cstring_buffer.len() as c_int;

    // 3. Prepare raw vector
    let mut c_char_buffer: Vec<*mut c_char> = Vec::new();
    for cstring in &cstring_buffer {
        c_char_buffer.push(cstring.as_bytes_with_nul().as_ptr() as *mut c_char);
    }
    c_char_buffer.push(ptr::null_mut());

    let c_argv = c_char_buffer.as_mut_ptr();

    // 4. Now call the function
    let ret = func(argc, c_argv) as i32;

    // 5. Write back modifications
    if let Some(mod_range) = modified_range(ret) {
        for (&c_char, string) in c_char_buffer[mod_range.clone()]
            .iter()
            .zip(&mut args[mod_range])
        {
            assert!(!c_char.is_null(), "func() had put NULL into argv");
            *string = std::ffi::CStr::from_ptr(c_char)
                .to_str()
                .unwrap()
                .to_owned();
        }
    }

    ret
}
