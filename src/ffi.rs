extern crate libc;

use self::libc::{c_void, c_char, c_int, pipe2, O_NONBLOCK, read, write, close};
use std::ffi::{CString, CStr};
use std::panic::{UnwindSafe, catch_unwind};
use discord::model::ChannelId;
use {ConnectionState, run_command, input};

pub struct Buffer {
    ptr: *mut c_void,
}

pub const MAIN_BUFFER: Buffer = Buffer { ptr: 0 as *mut c_void };

pub struct Completion {
    ptr: *mut c_void,
}

pub struct Hook {
    ptr: *mut c_void,
}

impl Drop for Hook {
    fn drop(&mut self) {
        extern "C" {
            fn wdc_unhook(hook: *mut c_void);
        }
        unsafe {
            wdc_unhook(self.ptr);
        }
    }
}

pub struct WeechatAny {
    data: *mut c_void,
    hdata: *mut c_void,
}

impl PartialEq for WeechatAny {
    fn eq(&self, rhs: &Self) -> bool {
        self.ptr() == rhs.ptr()
    }
}


pub struct SharedString(pub String);

fn strip_indexer_field(field: &str) -> &str {
    if let Some(idx) = field.find('|') {
        &field[(idx + 1)..]
    } else {
        field
    }
}

pub trait WeechatObject {
    fn from_ptr_hdata(ptr: *mut c_void, hdata: *mut c_void) -> Self;
    fn ptr(&self) -> *mut c_void;
    fn hdata(&self) -> *mut c_void;

    fn get<T: HDataGetResult>(&self, field: &str) -> Option<T> {
        let field_type = T::weechat_type();
        if field_type != "" {
            let actual_type = hdata_get_var_type_string(self.hdata(), field);
            if field_type != actual_type {
                really_bad(format!("Field {} had type {} but we expected {}",
                                   field,
                                   actual_type,
                                   field_type));
            }
        }
        T::new::<Self>(&self, &field)
    }

    fn get_idx<T: HDataGetResult>(&self, field: &str, index: usize) -> Option<T> {
        self.get(&format!("{}|{}", index, field))
    }

    fn get_any(&self, field: &str) -> Option<WeechatAny> {
        self.get(field)
    }
}

impl WeechatObject for WeechatAny {
    fn from_ptr_hdata(data: *mut c_void, hdata: *mut c_void) -> Self {
        WeechatAny {
            data: data,
            hdata: hdata,
        }
    }

    fn ptr(&self) -> *mut c_void {
        self.data
    }

    fn hdata(&self) -> *mut c_void {
        self.hdata
    }
}

pub trait HDataGetResult: Sized {
    fn new<T: WeechatObject + ?Sized>(parent: &T, field: &str) -> Option<Self>;
    fn weechat_type() -> &'static str;
}

impl<T: WeechatObject> HDataGetResult for T {
    fn new<P: WeechatObject + ?Sized>(parent: &P, field: &str) -> Option<Self> {
        let data = try_opt!(hdata_pointer(parent.hdata(), parent.ptr(), field));
        let hdata_name = hdata_get_var_hdata(parent.hdata(), field);
        let hdata = hdata_get(&hdata_name);
        Some(Self::from_ptr_hdata(data, hdata))
    }

    fn weechat_type() -> &'static str {
        "pointer"
    }
}

impl HDataGetResult for String {
    fn new<P: WeechatObject + ?Sized>(parent: &P, field: &str) -> Option<Self> {
        hdata_string(parent.hdata(), parent.ptr(), field)
    }

    fn weechat_type() -> &'static str {
        "string"
    }
}

impl HDataGetResult for SharedString {
    fn new<P: WeechatObject + ?Sized>(parent: &P, field: &str) -> Option<Self> {
        hdata_string(parent.hdata(), parent.ptr(), field).map(SharedString)
    }

    fn weechat_type() -> &'static str {
        "shared_string"
    }
}

impl HDataGetResult for i32 {
    fn new<P: WeechatObject + ?Sized>(parent: &P, field: &str) -> Option<Self> {
        hdata_integer(parent.hdata(), parent.ptr(), field).map(|x| x as i32)
    }

    fn weechat_type() -> &'static str {
        "integer"
    }
}

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

pub fn really_bad(message: String) -> ! {
    MAIN_BUFFER.print(&format!("{}: Internal error - {}", ::weechat::COMMAND, message));
    panic!(message); // hopefully we hit a catch_unwind
}

impl Buffer {
    pub fn new(name: &str, channel: &ChannelId) -> Option<Buffer> {
        extern "C" {
            fn wdc_buffer_new(name: *const c_char, data: *const c_char) -> *mut c_void;
        }
        unsafe {
            let name = unwrap1!(CString::new(name));
            let id = format!("{}", channel.0);
            let id = unwrap1!(CString::new(id));
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
            fn wdc_buffer_search(name: *const c_char) -> *mut c_void;
        }
        unsafe {
            let name_c = unwrap1!(CString::new(name));
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
            fn wdc_print(buffer: *mut c_void, message: *const c_char);
        }
        unsafe {
            let msg = unwrap1!(CString::new(message));
            wdc_print(self.ptr, msg.as_ptr());
        }
    }

    pub fn print_tags(&self, tags: &str, message: &str) {
        extern "C" {
            fn wdc_print_tags(buffer: *mut c_void, tags: *const c_char, message: *const c_char);
        }
        unsafe {
            let msg = unwrap1!(CString::new(message));
            let tags = unwrap1!(CString::new(tags));
            wdc_print_tags(self.ptr, tags.as_ptr(), msg.as_ptr());
        }
    }

    pub fn load_weechat_backlog(&self) {
        extern "C" {
            fn wdc_load_backlog(sig_data: *mut c_void);
        }
        unsafe {
            wdc_load_backlog(self.ptr as *mut c_void);
        }
    }

    pub fn set(&self, property: &str, value: &str) {
        extern "C" {
            fn wdc_buffer_set(buffer: *mut c_void, property: *const c_char, value: *const c_char);
        }
        unsafe {
            let property = unwrap1!(CString::new(property));
            let value = unwrap1!(CString::new(value));
            wdc_buffer_set(self.ptr, property.as_ptr(), value.as_ptr());
        }
    }

    pub fn add_nick(&self, nick: &str) {
        extern "C" {
            fn wdc_nicklist_add_nick(buffer: *const c_void, nick: *const c_char);
        }
        unsafe {
            let nick = CString::new(nick).unwrap();
            wdc_nicklist_add_nick(self.ptr, nick.as_ptr());
        }
    }

    pub fn remove_nick(&self, nick: &str) {
        extern "C" {
            fn wdc_nicklist_remove_nick(buffer: *const c_void, nick: *const c_char);
        }
        unsafe {
            let nick = CString::new(nick).unwrap();
            wdc_nicklist_remove_nick(self.ptr, nick.as_ptr());
        }
    }
}

impl Completion {
    pub fn add(&mut self, word: &str) {
        extern "C" {
            fn wdc_hook_completion_add(gui_completion: *const c_void, word: *const c_char);
        }
        unsafe {
            let word_c = CString::new(word).unwrap();
            wdc_hook_completion_add(self.ptr, word_c.as_ptr());
        }
    }
}

impl WeechatObject for Buffer {
    fn from_ptr_hdata(ptr: *mut c_void, hdata: *mut c_void) -> Self {
        let result = Buffer { ptr: ptr };
        if hdata != result.hdata() {
            really_bad("Buffer hdata pointer was different!".into());
        };
        result
    }

    fn ptr(&self) -> *mut c_void {
        self.ptr
    }

    fn hdata(&self) -> *mut c_void {
        hdata_get("buffer")
    }
}

pub struct PokeableFd {
    _hook: Hook,
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
        // TODO: Check if hook is nil
        PokeableFd {
            _hook: Hook { ptr: hook },
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
        unsafe {
            close(self.pipe[0]);
            close(self.pipe[1]);
        }
    }
}

fn wrap_panic<F: FnOnce() -> () + UnwindSafe>(f: F) -> () {
    let result = catch_unwind(|| f());
    match result {
        Ok(()) => (),
        Err(err) => {
            let msg = match err.downcast_ref::<String>() {
                Some(msg) => msg,
                None => "unknown error",
            };
            let result = catch_unwind(|| {
                MAIN_BUFFER.print(&format!(
                    "{}: Fatal error (caught) - {}", ::weechat::COMMAND, msg))
            });
            let _ = result; // eat error without logging :(
        }
    }
}

#[no_mangle]
pub extern "C" fn wdr_end() {
    wrap_panic(|| drop_global_state());
}

#[no_mangle]
pub extern "C" fn wdr_init() {
    wrap_panic(|| ::init());
}

#[no_mangle]
pub unsafe extern "C" fn wdr_input(buffer: *mut c_void,
                                   channel_id: *const c_char,
                                   input_str: *const c_char) {
    wrap_panic(|| {
        let buffer = Buffer { ptr: buffer };
        let channel_id = ChannelId(unwrap1!(unwrap1!(CStr::from_ptr(channel_id).to_str()).parse()));
        let input_str = unwrap1!(CStr::from_ptr(input_str).to_str());
        let state = get_global_state();
        input(state, buffer, channel_id, input_str);
    });
}

#[no_mangle]
pub unsafe extern "C" fn wdr_command(buffer_c: *mut c_void, command_c: *const c_char) {
    wrap_panic(|| {
        let buffer = Buffer { ptr: buffer_c };
        let state = get_global_state();
        let command = unwrap1!(CStr::from_ptr(command_c).to_str());
        if run_command(buffer, state, command) == false {
            drop_global_state();
        }
    });
}

#[no_mangle]
pub extern "C" fn wdr_hook_fd_callback(callback: *const c_void, fd: c_int) {
    wrap_panic(|| {
        let func = unsafe {
            let mut tmp = 0 as c_char;
            while read(fd, (&mut tmp) as *mut c_char as *mut c_void, 1) == 1 {
            }
            &*(callback as *const fn(&'static mut ConnectionState))
        };
        let state = unwrap!(get_global_state());
        func(state);
    });
}

#[no_mangle]
pub extern "C" fn wdr_hook_completion_callback(callback: *const c_void,
                                               buffer: *mut c_void,
                                               completion: *mut c_void) {
    wrap_panic(|| {
        let func = unsafe {
            let callback_typed =
                callback as *const fn(&'static mut ConnectionState, Buffer, Completion);
            &*callback_typed
        };
        let state = unwrap!(get_global_state());
        let buffer = Buffer { ptr: buffer };
        let completion = Completion { ptr: completion };
        func(state, buffer, completion);
    });
}

pub fn hook_command(cmd: &str, desc: &str, args: &str, argdesc: &str, compl: &str) {
    extern "C" {
        fn wdc_hook_command(command: *const c_char,
                            description: *const c_char,
                            args: *const c_char,
                            args_description: *const c_char,
                            completion: *const c_char);
    }
    unsafe {
        let cmd = unwrap1!(CString::new(cmd));
        let desc = unwrap1!(CString::new(desc));
        let args = unwrap1!(CString::new(args));
        let argdesc = unwrap1!(CString::new(argdesc));
        let compl = unwrap1!(CString::new(compl));
        wdc_hook_command(cmd.as_ptr(),
                         desc.as_ptr(),
                         args.as_ptr(),
                         argdesc.as_ptr(),
                         compl.as_ptr());
    }
}

pub fn info_get(info_name: &str, arguments: &str) -> Option<String> {
    extern "C" {
        fn wdc_info_get(info_name: *const c_char, arguments: *const c_char) -> *const c_char;
    }
    unsafe {
        let info_name = unwrap1!(CString::new(info_name));
        let arguments = unwrap1!(CString::new(arguments));
        let result = wdc_info_get(info_name.as_ptr(), arguments.as_ptr());
        if result.is_null() {
            None
        } else {
            Some(CStr::from_ptr(result).to_string_lossy().into_owned())
        }
    }
}

fn hdata_get(name: &str) -> *mut c_void {
    extern "C" {
        fn wdc_hdata_get(name: *const c_char) -> *mut c_void;
    }
    unsafe {
        let name_c = unwrap1!(CString::new(name));
        let data = wdc_hdata_get(name_c.as_ptr());
        if data.is_null() {
            really_bad(format!("hdata name {} was invalid", name));
        }
        data
    }
}

fn hdata_pointer(hdata: *mut c_void, obj: *mut c_void, name: &str) -> Option<*mut c_void> {
    extern "C" {
        fn wdc_hdata_pointer(hdata: *mut c_void,
                             obj: *mut c_void,
                             name: *const c_char)
                             -> *mut c_void;
    }
    unsafe {
        let name = unwrap1!(CString::new(name));
        let result = wdc_hdata_pointer(hdata, obj, name.as_ptr());
        if result.is_null() { None } else { Some(result) }
    }
}

fn hdata_get_var_hdata(hdata: *mut c_void, name: &str) -> String {
    extern "C" {
        fn wdc_hdata_get_var_hdata(hdata: *mut c_void, name: *const c_char) -> *const c_char;
    }
    let name = strip_indexer_field(name);
    unsafe {
        let name_c = unwrap1!(CString::new(name));
        let result = wdc_hdata_get_var_hdata(hdata, name_c.as_ptr());
        if result.is_null() {
            really_bad(format!("hdata field {} hdata was invalid", name));
        }
        CStr::from_ptr(result).to_string_lossy().into_owned()
    }
}

fn hdata_get_var_type_string(hdata: *mut c_void, name: &str) -> String {
    extern "C" {
        fn wdc_hdata_get_var_type_string(hdata: *mut c_void, name: *const c_char) -> *const c_char;
    }
    let name = strip_indexer_field(name);
    unsafe {
        let name_c = unwrap1!(CString::new(name));
        let result = wdc_hdata_get_var_type_string(hdata, name_c.as_ptr());
        if result.is_null() {
            really_bad(format!("hdata field {} type was invalid", name));
        }
        CStr::from_ptr(result).to_string_lossy().into_owned()
    }
}

fn hdata_integer(hdata: *mut c_void, data: *mut c_void, name: &str) -> Option<c_int> {
    extern "C" {
        fn wdc_hdata_integer(hdata: *mut c_void, data: *mut c_void, name: *const c_char) -> c_int;
    }
    unsafe {
        let name = unwrap1!(CString::new(name));
        Some(wdc_hdata_integer(hdata, data, name.as_ptr()))
    }
}

fn hdata_string(hdata: *mut c_void, data: *mut c_void, name: &str) -> Option<String> {
    extern "C" {
        fn wdc_hdata_string(hdata: *mut c_void,
                            data: *mut c_void,
                            name: *const c_char)
                            -> *const c_char;
    }
    unsafe {
        let name = unwrap1!(CString::new(name));
        let result = wdc_hdata_string(hdata, data, name.as_ptr());
        if result.is_null() {
            None
        } else {
            Some(CStr::from_ptr(result).to_string_lossy().into_owned())
        }
    }
}

pub fn get_option(name: &str) -> Option<String> {
    extern "C" {
        fn wdc_config_get_plugin(name: *const c_char) -> *const c_char;
    }
    unsafe {
        let name_c = unwrap1!(CString::new(name));
        let result = wdc_config_get_plugin(name_c.as_ptr());
        if result.is_null() {
            None
        } else {
            Some(unwrap1!(CStr::from_ptr(result).to_str()).into())
        }
    }
}

pub fn set_option(name: &str, value: &str) -> String {
    extern "C" {
        fn wdc_config_set_plugin(name: *const c_char, value: *const c_char) -> c_int;
    }
    let before = get_option(name);
    let result = unsafe {
        let name_c = unwrap1!(CString::new(name));
        let value_c = unwrap1!(CString::new(value));
        wdc_config_set_plugin(name_c.as_ptr(), value_c.as_ptr())
    };
    match (result, before) {
        (0, Some(before)) => format!("option successfully changed from {} to {}", before, value),
        (0, None) | (1, None) => format!("option successfully set to {}", value),
        (1, Some(before)) => format!("option already contained {}", before),
        (2, _) => format!("option {} not found", name),
        (_, Some(before)) => {
            format!("error when setting option {} to {} (was {})",
                    name,
                    value,
                    before)
        }
        (_, None) => format!("error when setting option {} to {}", name, value),
    }
}

pub fn hook_completion(name: &str,
                       description: &str,
                       callback: &'static fn(&'static mut ConnectionState, Buffer, Completion))
                       -> Option<Hook> {
    extern "C" {
        fn wdc_hook_completion(completion_item: *const c_char,
                               description: *const c_char,
                               callback_pointer: *const c_void)
                               -> *mut c_void;
    }
    unsafe {
        let name_c = unwrap1!(CString::new(name));
        let description_c = unwrap1!(CString::new(description));
        let callback_ptr = callback as *const fn(&'static mut ConnectionState, Buffer, Completion);
        let callback_c = callback_ptr as *const c_void;
        let result = wdc_hook_completion(name_c.as_ptr(), description_c.as_ptr(), callback_c);
        if result.is_null() {
            None
        } else {
            Some(Hook { ptr: result })
        }
    }
}
