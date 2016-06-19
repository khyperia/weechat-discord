extern crate discord;
extern crate libc;

use libc::{c_void, c_char};
use std::ffi::CString;

extern {
    fn wdc_print_main(message: *const c_char);
}

#[no_mangle]
pub extern fn wdr_init() {
    let c_to_print = CString::new("Hello, Rust!").unwrap();
    unsafe {
        wdc_print_main(c_to_print.as_ptr());
    }
}

#[no_mangle]
pub extern fn wdr_end() {}

#[no_mangle]
pub extern fn wdr_command(buffer: *const c_void, command: *const c_char) {}

#[no_mangle]
pub extern fn wdr_input(buffer: *const c_void, channel_id: *const c_char, input: *const c_char) {}
