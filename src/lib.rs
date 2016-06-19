extern crate discord;
extern crate libc;

use libc::{c_void, c_char, c_int};
use std::ffi::{CString, CStr};
use std::collections::VecDeque;
use std::sync::Mutex;
use std::error::Error;
use discord::{Discord, Connection};
use discord::model::Event;

struct Buffer {
    ptr: *const c_void,
}

struct ConnectionState {
    discord: Discord,
    connection: Connection,
    events: Mutex<VecDeque<Event>>,
    pipe: [c_int; 2],
}

type ConnectionStateWrap = Option<ConnectionState>;

const MAIN_BUFFER: Buffer = Buffer { ptr: 0 as *const c_void };

//fn to_c_str(string: &str) -> *const c_char {
//    CString::new(string).unwrap().as_ptr()
//}

impl Buffer {
    fn print(&self, message: &str) {
        extern "C" {
            fn wdc_print(buffer: *const c_void, message: *const c_char);
        }
        unsafe {
            let msg = CString::new(message).unwrap();
            wdc_print(self.ptr, msg.as_ptr());
        }
    }
}

// make sure to mem::forget the result of this!
fn get_crazy(raw: *const c_void) -> Box<ConnectionStateWrap> {
    unsafe { Box::from_raw(raw as *mut Option<ConnectionState>) }
}

#[no_mangle]
pub extern "C" fn wdr_init() {
    extern "C" {
        fn wdc_hook_command(command: *const c_char,
                            description: *const c_char,
                            args: *const c_char,
                            args_description: *const c_char,
                            completion: *const c_char,
                            callback_pointer: *const c_void);
    }
    MAIN_BUFFER.print("Hello, Rust!");
    let state: Box<ConnectionStateWrap> = Box::new(None);
    unsafe {
        let cmd = CString::new("discord").unwrap();
        let desc = CString::new("Confdsa").unwrap();
        let args = CString::new("").unwrap();
        let argdesc = CString::new("").unwrap();
        let compl = CString::new("").unwrap();
        wdc_hook_command(cmd.as_ptr(), desc.as_ptr(), args.as_ptr(), argdesc.as_ptr(), compl.as_ptr(),
                         Box::into_raw(state) as *const c_void);
    }
    // state is moved via into_raw, equivalent to mem::forget
}

#[no_mangle]
pub extern "C" fn wdr_end() {
    // TODO: Kill/join worker and drop state
}

#[no_mangle]
pub unsafe extern "C" fn wdr_command(buffer_c: *const c_void,
                                     state_c: *const c_void,
                                     command_c: *const c_char) {
    let buffer = Buffer { ptr: buffer_c };
    let mut state = get_crazy(state_c);
    let command = CStr::from_ptr(command_c).to_str().unwrap();
    run_command(buffer, &mut *state, command);
    std::mem::forget(state);
}

#[no_mangle]
pub extern "C" fn wdr_input(buffer: *const c_void,
                            channel_id: *const c_char,
                            input: *const c_char,
                            state_c: *const c_void) {
    let _ = buffer;
    let _ = channel_id;
    let _ = input;
    let _ = state_c;
}

#[no_mangle]
pub extern "C" fn wdr_hook_fd_callback(state_c: *const c_void, fd: c_int) {
    let mut tmp = 0 as c_char;
    unsafe {while libc::read(fd, (&mut tmp) as *mut c_char as *mut c_void, 1) == 1 {
        MAIN_BUFFER.print("Unpoke!");
    }}
    let mut state = get_crazy(state_c);
    process_events(&mut *state);
    std::mem::forget(state);
}

fn set_option(name: &str, value: &str) -> String {
    extern "C" {
        fn wdc_config_set_plugin(name: *const c_char, value: *const c_char) -> c_int;
    }
    let before = get_option(name);
    let result = unsafe {
            let name_c = CString::new(name).unwrap();
            let value_c = CString::new(value).unwrap();
        wdc_config_set_plugin(name_c.as_ptr(), value_c.as_ptr())
    };
    match (result, before) {
        (0, Some(before)) => format!("Option successfully changed from {} to {}", before, value),
        (0, None) | (1, None) => format!("Option successfully set to {}", value),
        (1, Some(before)) => format!("Option already contained {}", before),
        (2, _) => format!("Option {} not found", name),
        (_, Some(before)) => {
            format!("Error when setting option {} to {} (was {})",
                    name,
                    value,
                    before)
        }
        (_, None) => format!("Error when setting option {} to {}", name, value),
    }
}

fn get_option(name: &str) -> Option<String> {
    extern "C" {
        fn wdc_config_get_plugin(name: *const c_char) -> *const c_char;
    }
    unsafe {
        let name_c = CString::new(name).unwrap();
        let result = wdc_config_get_plugin(name_c.as_ptr());
        if result.is_null() {
            None
        } else {
            Some(CStr::from_ptr(result).to_str().unwrap().into())
        }
    }
}

fn user_set_option(buffer: Buffer, name: &str, value: &str) {
    buffer.print(&set_option(name, value));
}

fn connect(buffer: Buffer, state: &mut ConnectionStateWrap) {
    let (email, password) = match (get_option("email"), get_option("password")) {
        (Some(e), Some(p)) => (e, p),
        (email, password) => {
            buffer.print("Error: plugins.var.weecord.{email,password} unset. Run:");
            if email.is_none() {
                buffer.print("/discord email your.email@example.com");
            }
            if password.is_none() {
                buffer.print("/discord password hunter2");
            }
            return;
        }
    };
    buffer.print("Discord: Connecting");
    let discord = match Discord::new(&email, &password) {
        Ok(discord) => discord,
        Err(err) => {
            buffer.print(&format!("Connection error: {}", err.description()));
            return;
        }
    };
    let (connection, ready) = match discord.connect() {
        Ok(ok) => ok,
        Err(err) => {
            buffer.print(&format!("Connection error: {}", err.description()));
            return;
        }
    };
    let mut pipe: [c_int; 2] = [0; 2];
    unsafe {
        extern "C" {
            fn wdc_hook_fd(fd: c_int, pointer: *const c_void) -> c_int;
        }
        libc::pipe2(&mut pipe[0] as &mut c_int as *mut c_int, libc::O_NONBLOCK);
        // TODO: This might need a box?
        wdc_hook_fd(pipe[0], state as *mut ConnectionStateWrap as *mut c_void);
    }

    // TODO: on_ready
    buffer.print("Discord: Connected");
    {
        *state = Some(ConnectionState {
            discord: discord,
            connection: connection,
            events: Mutex::new(VecDeque::new()),
            pipe: pipe,
        });
    }
    // say "haha screw you" to the borrow checker
    let state = match *state { Some(ref mut x) => x, None => panic!("Impossible") };
    let mut state = unsafe { Box::from_raw(state as *mut ConnectionState) };
    std::thread::spawn(move || {
        while let Ok(event) = state.connection.recv_event() {
            {
                let locked = state.events.get_mut().unwrap();
                locked.push_back(event);
            }
            unsafe {
                MAIN_BUFFER.print("Poke!");
                libc::write(state.pipe[1], &(0 as c_char) as *const c_char as *const c_void, 1);
            }
        }
        std::mem::forget(state);
    });
}

fn run_command(buffer: Buffer, state: &mut ConnectionStateWrap, command: &str) {
    if command == "connect" {
        connect(buffer, state);
    } else if command.starts_with("email ") {
        user_set_option(buffer, "email", &command["email ".len()..]);
    } else if command.starts_with("password ") {
        user_set_option(buffer, "password", &command["password ".len()..]);
    } else {

    }
}

fn process_events(state_opt: &mut ConnectionStateWrap) {
    let state = match *state_opt {
        Some(ref mut s) => s,
        None => {
            MAIN_BUFFER.print("safdsa");
            return;
        }
    };
    let locked = state.events.get_mut().unwrap();
    while let Some(event) = locked.pop_front() {
        if let discord::model::Event::MessageCreate(message) = event {
            MAIN_BUFFER.print(&message.content);
        }
    }
}
