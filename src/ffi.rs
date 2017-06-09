use libc::*;
use std::ffi::*;
use std::panic::*;

#[derive(PartialEq, Eq, Hash)]
pub struct Buffer {
    ptr: *mut c_void,
}

pub const MAIN_BUFFER: Buffer = Buffer { ptr: 0 as *mut c_void };

/*
pub struct Completion {
    ptr: *mut c_void,
}
*/

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
        T::new::<Self>(self, field)
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
        let data = hdata_pointer(parent.hdata(), parent.ptr(), field);
        let data = match data {
            Some(data) => data,
            None => return None,
        };
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

pub fn really_bad(message: String) -> ! {
    MAIN_BUFFER.print(&format!("{}: Internal error - {}", ::weechat::COMMAND, message));
    panic!(message); // hopefully we hit a catch_unwind
}

pub trait BufferImpl {
    fn input(&self, buffer: Buffer, message: &str);
    fn close(&self, buffer: Buffer);
}

impl Buffer {
    pub fn new(name: &str, buffer_impl: Box<BufferImpl>) -> Option<Buffer> {
        extern "C" {
            fn wdc_buffer_new(name: *const c_char,
                              pointer: *const c_void,
                              input_callback: extern "C" fn(*const c_void,
                                                            *mut c_void,
                                                            *mut c_void,
                                                            *const c_char)
                                                            -> c_int,
                              close_callback: extern "C" fn(*const c_void,
                                                            *mut c_void,
                                                            *mut c_void)
                                                            -> c_int)
                              -> *mut c_void;
        }
        extern "C" fn input_cb(pointer: *const c_void,
                               data: *mut c_void,
                               buffer: *mut c_void,
                               input_data: *const c_char)
                               -> c_int {
            let _ = data;
            wrap_panic(|| {
                let buffer = Buffer { ptr: buffer };
                let pointer = pointer as *const Box<BufferImpl>;
                let input_data = unsafe { CStr::from_ptr(input_data).to_str() };
                let input_data = match input_data {
                    Ok(x) => x,
                    Err(_) => return,
                };
                unsafe { &*pointer }.input(buffer, input_data);
            });
            0
        }
        extern "C" fn close_cb(pointer: *const c_void,
                               data: *mut c_void,
                               buffer: *mut c_void)
                               -> c_int {
            let _ = data;
            wrap_panic(|| {
                           let buffer = Buffer { ptr: buffer };
                           let pointer = pointer as *mut Box<BufferImpl>;
                           let data = unsafe { Box::from_raw(pointer) };
                           data.close(buffer);
                       });
            0
        }
        unsafe {
            let name = unwrap1!(CString::new(name));
            let pointer: Box<Box<BufferImpl>> = Box::new(buffer_impl);
            let pointer = Box::into_raw(pointer) as *mut c_void;
            let result = wdc_buffer_new(name.as_ptr(), pointer, input_cb, close_cb);
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

    /*
    pub fn load_weechat_backlog(&self) {
        extern "C" {
            fn wdc_load_backlog(sig_data: *mut c_void);
        }
        unsafe {
            wdc_load_backlog(self.ptr as *mut c_void);
        }
    }
    */

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

    pub fn nick_exists(&self, nick: &str) -> bool {
        extern "C" {
            fn wdc_nicklist_nick_exists(buffer: *const c_void, nick: *const c_char) -> c_int;
        }
        unsafe {
            let nick = CString::new(nick).unwrap();
            wdc_nicklist_nick_exists(self.ptr, nick.as_ptr()) != 0
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

    /*
    pub fn remove_nick(&self, nick: &str) {
        extern "C" {
            fn wdc_nicklist_remove_nick(buffer: *const c_void, nick: *const c_char);
        }
        unsafe {
            let nick = CString::new(nick).unwrap();
            wdc_nicklist_remove_nick(self.ptr, nick.as_ptr());
        }
    }
    */
}

/*
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
*/

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
    _callback: Box<Box<FnMut()>>,
}

pub struct PokeableFdPoker {
    fd: c_int,
}

impl PokeableFd {
    pub fn new<F: FnMut() + 'static>(callback: F) -> PokeableFd {
        extern "C" {
            fn wdc_hook_fd(fd: c_int,
                           pointer: *const c_void,
                           callback: extern "C" fn(*const c_void, *mut c_void, c_int) -> c_int)
                           -> *mut c_void;
        }
        extern "C" fn callback_fn(pointer: *const c_void, data: *mut c_void, fd: c_int) -> c_int {
            let _ = data;
            let mut tmp = 0 as c_char;
            unsafe { while read(fd, (&mut tmp) as *mut c_char as *mut c_void, 1) == 1 {} }
            wrap_panic(|| {
                           let callback = pointer as *mut Box<FnMut()>;
                           (unsafe { &mut **callback })();
                       });
            0
        }
        let mut pipe_fds = [0; 2];
        unsafe {
            pipe(&mut pipe_fds[0] as *mut c_int);
            // O_NONBLOCK is used in callback_fn while draining the pipe
            fcntl(pipe_fds[0],
                  F_SETFL,
                  fcntl(pipe_fds[0], F_GETFL) | O_NONBLOCK);
        }
        let callback: Box<Box<FnMut()>> = Box::new(Box::new(callback));
        let hook = unsafe {
            // haha screw you borrowck
            let callback = &*callback as *const _ as *const c_void;
            let hook = wdc_hook_fd(pipe_fds[0], callback, callback_fn);
            Hook { ptr: hook }
        };
        // TODO: Check if hook is nil
        PokeableFd {
            _hook: hook,
            pipe: pipe_fds,
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
    let result = catch_unwind(f);
    match result {
        Ok(()) => (),
        Err(err) => {
            let msg = match err.downcast_ref::<String>() {
                Some(msg) => msg,
                None => "unknown error",
            };
            let result = catch_unwind(|| {
                                          MAIN_BUFFER
                                              .print(&format!("{}: Fatal error (caught) - {}",
                                                              ::weechat::COMMAND,
                                                              msg))
                                      });
            let _ = result; // eat error without logging :(
        }
    }
}

#[no_mangle]
#[allow(unused)]
pub extern "C" fn wdr_end() {
    // wrap_panic(drop_global_state);
}

#[no_mangle]
#[allow(unused)]
pub extern "C" fn wdr_init() {
    // TODO
    wrap_panic(::init);
}

pub fn hook_command<F: FnMut(Buffer, &str) + 'static>(cmd: &str,
                                                      desc: &str,
                                                      args: &str,
                                                      argdesc: &str,
                                                      compl: &str,
                                                      func: F)
                                                      -> Option<Hook> {
    type CB = FnMut(Buffer, &str);
    extern "C" {
        fn wdc_hook_command(command: *const c_char,
                            description: *const c_char,
                            args: *const c_char,
                            args_description: *const c_char,
                            completion: *const c_char,
                            pointer: *const c_void,
                            callback: extern "C" fn(*const c_void,
                                                    *mut c_void,
                                                    *mut c_void,
                                                    c_int,
                                                    *mut *mut c_char,
                                                    *mut *mut c_char)
                                                    -> c_int)
                            -> *mut c_void;
    }
    extern "C" fn callback(pointer: *const c_void,
                           data: *mut c_void,
                           buffer: *mut c_void,
                           argc: c_int,
                           argv: *mut *mut c_char,
                           argv_eol: *mut *mut c_char)
                           -> c_int {
        let _ = data;
        let _ = argv;
        wrap_panic(|| {
            let pointer = pointer as *mut Box<CB>;
            let buffer = Buffer { ptr: buffer };
            if argc <= 1 {
                (unsafe { &mut **pointer })(buffer, "");
                return;
            }
            let args = unsafe { *argv_eol.offset(1) };
            let args = unsafe { CStr::from_ptr(args).to_str() };
            let args = match args {
                Ok(x) => x,
                Err(_) => return,
            };
            (unsafe { &mut **pointer })(buffer, args);
        });
        0
    }
    unsafe {
        let cmd = unwrap1!(CString::new(cmd));
        let desc = unwrap1!(CString::new(desc));
        let args = unwrap1!(CString::new(args));
        let argdesc = unwrap1!(CString::new(argdesc));
        let compl = unwrap1!(CString::new(compl));
        let pointer: Box<Box<CB>> = Box::new(Box::new(func));
        let pointer = Box::into_raw(pointer) as *const c_void; // TODO: Memory leak here.
        let hook = wdc_hook_command(cmd.as_ptr(),
                                    desc.as_ptr(),
                                    args.as_ptr(),
                                    argdesc.as_ptr(),
                                    compl.as_ptr(),
                                    pointer,
                                    callback);
        if hook.is_null() {
            None
        } else {
            Some(Hook { ptr: hook })
        }
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

/*
pub fn hook_completion<F: Fn(Buffer, Completion) + 'static>(name: &str,
                                                            description: &str,
                                                            callback: F)
                                                            -> Option<Hook> {
    type CB = Fn(Buffer, Completion);
    extern "C" {
        fn wdc_hook_completion(completion_item: *const c_char,
                               description: *const c_char,
                               callback_pointer: *const c_void,
                               callback: extern "C" fn(*const c_void,
                                                       *mut c_void,
                                                       *const c_char,
                                                       *mut c_void,
                                                       *mut c_void)
                                                       -> c_int)
                               -> *mut c_void;
    }
    extern "C" fn callback_func(pointer: *const c_void,
                                data: *mut c_void,
                                completion_item: *const c_char,
                                buffer: *mut c_void,
                                completion: *mut c_void)
                                -> c_int {
        let _ = data;
        let _ = completion_item;
        wrap_panic(|| {
                       let buffer = Buffer { ptr: buffer };
                       let completion = Completion { ptr: completion };
                       let pointer = pointer as *const Box<CB>;
                       (unsafe { &**pointer })(buffer, completion);
                   });
        0
    }
    let callback: Box<Box<CB>> = Box::new(Box::new(callback));
    unsafe {
        let name_c = unwrap1!(CString::new(name));
        let description_c = unwrap1!(CString::new(description));
        let callback = Box::into_raw(callback) as *const _ as *const c_void; // TODO: Memory leak
        let result = wdc_hook_completion(name_c.as_ptr(),
                                         description_c.as_ptr(),
                                         callback,
                                         callback_func);
        if result.is_null() {
            None
        } else {
            Some(Hook { ptr: result })
        }
    }
}
*/
