extern crate discord;
extern crate libc;

use libc::{c_void, c_char, c_int};
use std::ffi::{CString, CStr};
use std::collections::VecDeque;
use std::sync::Mutex;
use std::error::Error;
use discord::{Discord, Connection, State, ChannelRef};
use discord::model::{Event, ChannelId, ServerId, RoleId, User};

macro_rules! try_opt {
    ($expr:expr) => (match $expr { Some(e) => e, None => return None })
}

macro_rules! try_opt_ref {
    ($expr:expr) => (match *$expr { Some(ref e) => e, None => return None })
}

struct Buffer {
    ptr: *const c_void,
}

struct ConnectionState {
    discord: Discord,
    state: State,
    connection: Connection,
    events: Mutex<VecDeque<Event>>,
    pipe: [c_int; 2],
}

type ConnectionStateWrap = Option<ConnectionState>;

const MAIN_BUFFER: Buffer = Buffer { ptr: 0 as *const c_void };

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

    fn print_tags(&self, tags: &str, message: &str) {
        extern "C" {
            fn wdc_print_tags(buffer: *const c_void, tags: *const c_char, message: *const c_char);
        }
        unsafe {
            let msg = CString::new(message).unwrap();
            let tags = CString::new(tags).unwrap();
            wdc_print_tags(self.ptr, tags.as_ptr(), msg.as_ptr());
        }
    }

    fn load_backlog(&self) {
        extern "C" {
            fn wdc_load_backlog(sig_data: *mut c_void);
        }
        unsafe {
            wdc_load_backlog(self.ptr as *mut c_void);
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
        wdc_hook_command(cmd.as_ptr(),
                         desc.as_ptr(),
                         args.as_ptr(),
                         argdesc.as_ptr(),
                         compl.as_ptr(),
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
pub unsafe extern "C" fn wdr_input(buffer: *const c_void,
                                   channel_id: *const c_char,
                                   input_str: *const c_char,
                                   state_c: *const c_void) {
    let buffer = Buffer { ptr: buffer };
    let channel_id = ChannelId(CStr::from_ptr(channel_id).to_str().unwrap().parse().unwrap());
    let input_str = CStr::from_ptr(input_str).to_str().unwrap();
    let mut state = get_crazy(state_c);
    input(&mut *state, buffer, &channel_id, input_str);
    std::mem::forget(state);
}

#[no_mangle]
pub extern "C" fn wdr_hook_fd_callback(state_c: *const c_void, fd: c_int) {
    let mut tmp = 0 as c_char;
    unsafe {
        while libc::read(fd, (&mut tmp) as *mut c_char as *mut c_void, 1) == 1 {
            MAIN_BUFFER.print("Unpoke!");
        }
    }
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

fn buffer_search(name: &str) -> Option<Buffer> {
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

fn buffer_new(state: &mut ConnectionStateWrap,
              name: &str,
              channel_id: &ChannelId)
              -> Option<Buffer> {
    extern "C" {
        fn wdc_buffer_new(name: *const c_char,
                          pointer: *const c_void,
                          data: *const c_char)
                          -> *const c_void;
    }
    unsafe {
        let name = CString::new(name).unwrap();
        let state = state as *mut ConnectionStateWrap as *mut c_void;
        let id = format!("{}", channel_id.0);
        let id = CString::new(id).unwrap();
        let result = wdc_buffer_new(name.as_ptr(), state, id.as_ptr());
        if result.is_null() {
            None
        } else {
            Some(Buffer { ptr: result })
        }

    }
}

fn buffer_set(buffer: &Buffer, property: &str, value: &str) {
    extern "C" {
        fn wdc_buffer_set(buffer: *const c_void, property: *const c_char, value: *const c_char);
    }
    unsafe {
        let property = CString::new(property).unwrap();
        let value = CString::new(value).unwrap();
        wdc_buffer_set(buffer.ptr, property.as_ptr(), value.as_ptr());
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
    let dis_state = State::new(ready);
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
            state: dis_state,
            connection: connection,
            events: Mutex::new(VecDeque::new()),
            pipe: pipe,
        });
    }
    let state = match *state {
        Some(ref mut x) => x,
        None => panic!("Impossible"),
    };
    // say "haha screw you" to the borrow checker
    let mut state = unsafe { Box::from_raw(state as *mut ConnectionState) };
    std::thread::spawn(move || {
        while let Ok(event) = state.connection.recv_event() {
            state.state.update(&event);
            {
                let locked = state.events.get_mut().unwrap();
                locked.push_back(event);
            }
            unsafe {
                //MAIN_BUFFER.print("Poke!");
                libc::write(state.pipe[1],
                            &(0 as c_char) as *const c_char as *const c_void,
                            1);
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

fn input(state: &mut ConnectionStateWrap, buffer: Buffer, channel_id: &ChannelId, message: &str) {
    let _ = state;
    let _ = buffer;
    let _ = channel_id;
    let _ = message;
    // TODO: impl
}

fn process_events(state: &mut ConnectionStateWrap) {
    while let Some(event) = match *state {
        Some(ref mut s) => s.events.get_mut().unwrap().pop_front(),
        None => {
            MAIN_BUFFER.print("safdsa"); // TODO: what
            return;
        }
    } {
        if let discord::model::Event::MessageCreate(message) = event {
            // TODO: message.mention_roles
            let is_self = is_self_mentioned(state,
                                            &message.channel_id,
                                            message.mention_everyone,
                                            Some(message.mentions),
                                            None);
            display(state,
                    &message.content,
                    &message.channel_id,
                    Some(message.author),
                    is_self);
            MAIN_BUFFER.print(&message.content);
        }
    }
}

fn get_buffer(state: &mut ConnectionStateWrap, channel_id: &ChannelId) -> Option<Buffer> {
    let (server_name, channel_name, server_id, channel_id) = {
        let channel = try_opt!(try_opt_ref!(state).state.find_channel(channel_id));
        match channel {
            ChannelRef::Private(ch) => {
                ("discord-pm".into(), ch.recipient.name.clone(), ServerId(0), ch.id)
            }
            ChannelRef::Public(srv, ch) => {
                (srv.name.clone(), format!("#{}", ch.name), srv.id, ch.id)
            }
        }
    };
    let buffer_id = format!("{}.{}", server_id.0, channel_id.0);
    let buffer_name = format!("{} {}", server_name, channel_name);
    let buffer = match buffer_search(&buffer_id) {
        Some(buffer) => buffer,
        None => {
            let buffer = try_opt!(buffer_new(state, &buffer_id, &channel_id));
            buffer_set(&buffer, "short_name", &buffer_name);
            buffer_set(&buffer, "title", "Channel Title");
            buffer_set(&buffer, "type", "formatted");
            buffer_set(&buffer, "nicklist", "1");
            buffer.load_backlog();
            buffer
        }
    };
    Some(buffer)
}

fn is_self_mentioned(state: &mut ConnectionStateWrap,
                     channel_id: &ChannelId,
                     mention_everyone: bool,
                     mentions: Option<Vec<User>>,
                     roles: Option<Vec<RoleId>>)
                     -> bool {
    if mention_everyone {
        return true;
    }
    let state = match *state {
        Some(ref mut x) => x,
        None => return false,
    };
    let me = state.state.user();
    if let Some(mentions) = mentions {
        for mention in mentions {
            if me.id == mention.id {
                return true;
            }
        }
    }
    let server = state.state.find_channel(channel_id).and_then(|channel| match channel {
        ChannelRef::Public(server, _) => Some(server),
        _ => None,
    });
    if let (Some(roles), Some(server)) = (roles, server) {
        for role in roles {
            for member in &server.members {
                if member.user.id == me.id {
                    for member_role in &member.roles {
                        if member_role.0 == role.0 {
                            return true;
                        }
                    }
                }
            }
        }
    }
    return false;
}

fn display(state: &mut ConnectionStateWrap,
           content: &str,
           channel_id: &ChannelId,
           author: Option<User>,
           self_mentioned: bool) {
    let buffer = match get_buffer(state, channel_id) {
        Some(buffer) => buffer,
        None => return,
    };
    // TODO: Replace mentions
    let mut tags = Vec::new();
    tags.push(if self_mentioned {
            "notify_highlight"
        } else {
            "notify_message"
        }
        .into());
    let name = author.map_or("[unknown]".into(), |x| x.name.replace(',', ""));
    tags.push(format!("nick_{}", name));
    // nick_color
    buffer.print_tags(&tags.join(",".into()), &format!("{}\t{}", name, content));
}
