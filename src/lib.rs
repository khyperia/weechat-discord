extern crate discord;
extern crate libc;

#[macro_use]
mod macros;
pub mod ffi;

use libc::{c_char, c_int};
use std::ffi::{CString, CStr};
use std::collections::VecDeque;
use std::sync::Mutex;
use std::error::Error;
use discord::{Discord, Connection, State, ChannelRef};
use discord::model::{Event, ChannelId, ServerId, RoleId, User};
use ffi::{Buffer, MAIN_BUFFER, PokeableFd, get_global_state, set_global_state};

pub struct ConnectionState {
    discord: Discord,
    state: State,
    connection: Connection,
    events: Mutex<VecDeque<Event>>,
    pipe: PokeableFd,
}

#[no_mangle]
pub extern "C" fn wdr_end() {
    // TODO: Kill/join worker and drop state
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

fn connect(buffer: Buffer) {
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

    // TODO: on_ready
    buffer.print("Discord: Connected");
    let _ = set_global_state(ConnectionState {
        discord: discord,
        state: dis_state,
        connection: connection,
        events: Mutex::new(VecDeque::new()),
        pipe: PokeableFd::new(Box::new(process_events)),
    });
    std::thread::spawn(move || {
        let state = get_global_state().unwrap();
        while let Ok(event) = state.connection.recv_event() {
            state.state.update(&event);
            {
                let locked = state.events.get_mut().unwrap();
                locked.push_back(event);
            }
            state.pipe.poke();
        }
        std::mem::forget(state);
    });
}

fn run_command(buffer: Buffer, state: Option<&'static mut ConnectionState>, command: &str) {
    let _ = state;
    if command == "connect" {
        connect(buffer);
    } else if command.starts_with("email ") {
        user_set_option(buffer, "email", &command["email ".len()..]);
    } else if command.starts_with("password ") {
        user_set_option(buffer, "password", &command["password ".len()..]);
    } else {

    }
}

fn input(state: Option<&'static mut ConnectionState>,
         buffer: Buffer,
         channel_id: &ChannelId,
         message: &str) {
    let _ = state;
    let _ = buffer;
    let _ = channel_id;
    let _ = message;
    // TODO: impl
}

fn process_events(state: &'static mut ConnectionState) {
    loop {
        let event = {
            let mut queue = state.events.lock().unwrap();
            match queue.pop_front() {
                Some(event) => event,
                None => return,
            }
        };
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

fn get_buffer(state: &'static ConnectionState, channel_id: &ChannelId) -> Option<Buffer> {
    let (server_name, channel_name, server_id, channel_id) = {
        let channel = try_opt!(state.state.find_channel(channel_id));
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
    let buffer = match Buffer::search(&buffer_id) {
        Some(buffer) => buffer,
        None => {
            let buffer = try_opt!(Buffer::new(&buffer_id, &channel_id));
            buffer.set("short_name", &buffer_name);
            buffer.set("title", "Channel Title");
            buffer.set("type", "formatted");
            buffer.set("nicklist", "1");
            buffer.load_backlog();
            buffer
        }
    };
    Some(buffer)
}

fn is_self_mentioned(state: &'static ConnectionState,
                     channel_id: &ChannelId,
                     mention_everyone: bool,
                     mentions: Option<Vec<User>>,
                     roles: Option<Vec<RoleId>>)
                     -> bool {
    if mention_everyone {
        return true;
    }
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

fn display(state: &'static ConnectionState,
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
