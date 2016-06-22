use libc::{c_void, c_char, c_int, pipe2, O_NONBLOCK, read, write, close};
use std::ffi::{CString, CStr};
use std::mem::forget;
use discord::model::ChannelId;
use {ConnectionState, run_command, input};

pub struct Buffer {
    ptr: *const c_void,
}

pub const MAIN_BUFFER: Buffer = Buffer { ptr: 0 as *const c_void };

static mut global_state: *mut ConnectionState = 0 as *mut ConnectionState;

fn get_global_state() -> Option<&'static mut ConnectionState> {
    unsafe { global_state.as_mut() }
}

pub fn set_global_state(state: ConnectionState) {
    unsafe {
        global_state = Box::into_raw(Box::new(state));
    }
}

fn drop_global_state() {
    unsafe {
        if global_state.is_null() {
            return;
        };
        Box::from_raw(global_state);
        global_state = 0 as *mut ConnectionState
    };
}

impl Buffer {
    pub fn new(name: &str, channel: &ChannelId) -> Option<Buffer> {
        extern "C" {
            fn wdc_buffer_new(name: *const c_char, data: *const c_char) -> *const c_void;
        }
        unsafe {
            let name = CString::new(name).unwrap();
            let id = format!("{}", channel.0);
            let id = CString::new(id).unwrap();
            let result = wdc_buffer_new(name.as_ptr(), id.as_ptr());
            if result.is_null() {
                None
            } else {
                Some(Buffer { ptr: result })
            }
        }
    }

    pub fn search(name: &str) -> Option<Buffer> {
        extern "C" {
            fn wdc_buffer_search(name: *const c_char) -> *const c_void;
        }
        unsafe {
            let name_c = CString::new(name).unwrap();
            let result = wdc_buffer_search(name_c.as_ptr());
            if result.is_null() {
                None
            } else {
                Some(Buffer { ptr: result })
            }
        }
    }

    pub fn print(&self, message: &str) {
        extern "C" {
            fn wdc_print(buffer: *const c_void, message: *const c_char);
        }
        unsafe {
            let msg = CString::new(message).unwrap();
            wdc_print(self.ptr, msg.as_ptr());
        }
    }

    pub fn print_tags(&self, tags: &str, message: &str) {
        extern "C" {
            fn wdc_print_tags(buffer: *const c_void, tags: *const c_char, message: *const c_char);
        }
        unsafe {
            let msg = CString::new(message).unwrap();
            let tags = CString::new(tags).unwrap();
            wdc_print_tags(self.ptr, tags.as_ptr(), msg.as_ptr());
        }
    }

    pub fn load_backlog(&self) {
        extern "C" {
            fn wdc_load_backlog(sig_data: *mut c_void);
        }
        unsafe {
            wdc_load_backlog(self.ptr as *mut c_void);
        }
    }

    pub fn set(&self, property: &str, value: &str) {
        extern "C" {
            fn wdc_buffer_set(buffer: *const c_void,
                              property: *const c_char,
                              value: *const c_char);
        }
        unsafe {
            let property = CString::new(property).unwrap();
            let value = CString::new(value).unwrap();
            wdc_buffer_set(self.ptr, property.as_ptr(), value.as_ptr());
        }
    }
}

pub struct PokeableFd {
    hook: *mut c_void, // struct t_hook*
    pipe: [c_int; 2],
    _callback: Box<fn(&'static mut ConnectionState)>,
}

pub struct PokeableFdPoker {
    fd: c_int,
}

impl PokeableFd {
    pub fn new(callback: Box<fn(&'static mut ConnectionState)>) -> PokeableFd {
        extern "C" {
            fn wdc_hook_fd(fd: c_int, pointer: *const c_void) -> *mut c_void;
        }
        let mut pipe = [0; 2];
        unsafe { pipe2(&mut pipe[0] as *mut c_int, O_NONBLOCK) };
        let hook = unsafe {
            // haha screw you borrowck
            let callback = &*callback as *const fn(&'static mut ConnectionState) as *const c_void;
            wdc_hook_fd(pipe[0], callback)
        };
        PokeableFd {
            hook: hook,
            pipe: pipe,
            _callback: callback,
        }
    }

    pub fn get_poker(&self) -> PokeableFdPoker {
        PokeableFdPoker { fd: self.pipe[1] }
    }
}

impl PokeableFdPoker {
    pub fn poke(&self) {
        unsafe {
            write(self.fd, &(0 as c_char) as *const c_char as *const c_void, 1);
        }
    }
}

impl Drop for PokeableFd {
    fn drop(&mut self) {
        extern "C" {
            fn wdc_unhook(hook: *mut c_void);
        }
        unsafe {
            wdc_unhook(self.hook);
            close(self.pipe[0]);
            close(self.pipe[1]);
        }
    }
}

#[no_mangle]
pub extern "C" fn wdr_end() {
    drop_global_state();
}

#[no_mangle]
pub extern "C" fn wdr_init() {
    extern "C" {
        fn wdc_hook_command(command: *const c_char,
                            description: *const c_char,
                            args: *const c_char,
                            args_description: *const c_char,
                            completion: *const c_char);
    }
    unsafe {
        let cmd = CString::new("discord").unwrap();
        let desc = CString::new("Confdsa").unwrap();
        let args = CString::new("").unwrap();
        let argdesc = CString::new("").unwrap();
        let compl = CString::new("").unwrap();
        wdc_hook_command(cmd.as_ptr(),
                         desc.as_ptr(),
                         args.as_ptr(),
                         argdesc.as_ptr(),
                         compl.as_ptr());
    }
}

#[no_mangle]
pub unsafe extern "C" fn wdr_input(buffer: *const c_void,
                                   channel_id: *const c_char,
                                   input_str: *const c_char) {
    let buffer = Buffer { ptr: buffer };
    let channel_id = ChannelId(CStr::from_ptr(channel_id).to_str().unwrap().parse().unwrap());
    let input_str = CStr::from_ptr(input_str).to_str().unwrap();
    let state = get_global_state();
    input(state, buffer, &channel_id, input_str);
}

#[no_mangle]
pub unsafe extern "C" fn wdr_command(buffer_c: *const c_void, command_c: *const c_char) {
    let buffer = Buffer { ptr: buffer_c };
    let state = get_global_state();
    let command = CStr::from_ptr(command_c).to_str().unwrap();
    if run_command(buffer, state, command) == false {
        drop_global_state();
    }
}

#[no_mangle]
pub extern "C" fn wdr_hook_fd_callback(callback: *const c_void, fd: c_int) {
    let func = unsafe {
        let mut tmp = 0 as c_char;
        while read(fd, (&mut tmp) as *mut c_char as *mut c_void, 1) == 1 {
            // MAIN_BUFFER.print("Unpoke!");
        }
        Box::from_raw(callback as *mut fn(&'static mut ConnectionState))
    };
    let state = get_global_state().unwrap();
    func(state);
    forget(func);
}
