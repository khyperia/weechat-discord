extern crate discord;
extern crate libc;

#[macro_use]
mod macros;
pub mod ffi;

use libc::{c_char, c_int};
use std::ffi::{CString, CStr};
use std::mem::drop;
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::error::Error;
use std::thread::spawn;
use discord::{Discord, State, ChannelRef};
use discord::model::{Event, ChannelId, ServerId, RoleId, User};
use ffi::{Buffer, MAIN_BUFFER, PokeableFd, set_global_state};

pub struct ConnectionState {
    _discord: Discord,
    state: State,
    events: Receiver<discord::Result<Event>>,
    _pipe: PokeableFd,
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
    let (mut connection, ready) = match discord.connect() {
        Ok(ok) => ok,
        Err(err) => {
            buffer.print(&format!("Connection error: {}", err.description()));
            return;
        }
    };
    let dis_state = State::new(ready);

    // TODO: on_ready (open buffers, etc)
    buffer.print("Discord: Connected");
    let (send, recv) = channel();
    let pipe = PokeableFd::new(Box::new(process_events));
    let pipe_poker = pipe.get_poker();
    let _ = set_global_state(ConnectionState {
        _discord: discord,
        state: dis_state,
        events: recv,
        _pipe: pipe,
    });
    spawn(move || {
        loop {
            let event = connection.recv_event();
            // note we want to send even if it's an error
            match (event.is_err(), send.send(event)) {
                // break if we failed to send, or got an error
                (true, _) | (_, Err(_)) => break,
                _ => (),
            };
            pipe_poker.poke();
        }
        drop(send);
        pipe_poker.poke();
    });
}

fn run_command(buffer: Buffer, state: Option<&mut ConnectionState>, command: &str) -> bool {
    let _ = state;
    if command == "connect" {
        connect(buffer);
    } else if command == "disconnect" {
        return false;
    } else if command.starts_with("email ") {
        user_set_option(buffer, "email", &command["email ".len()..]);
    } else if command.starts_with("password ") {
        user_set_option(buffer, "password", &command["password ".len()..]);
    } else {
        buffer.print("Discord: unknown command");
    }
    true
}

fn input(state: Option<&mut ConnectionState>,
         buffer: Buffer,
         channel_id: &ChannelId,
         message: &str) {
    let _ = state;
    let _ = buffer;
    let _ = channel_id;
    let _ = message;
    // TODO: impl
}

fn process_events(state: &mut ConnectionState) {
    loop {
        let event = state.events.try_recv();
        let event = match event {
            Ok(event) => event,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                MAIN_BUFFER.print("Discord: Listening thread stopped!");
                return;
            }
        };
        let event = match event {
            // TODO: Newer versions of Discord move this into Err
            Ok(discord::model::Event::Closed(err)) => {
                MAIN_BUFFER.print(&format!("Discord: listening thread closed with code - {}", err));
                continue;
            }
            Ok(event) => event,
            Err(err) => {
                MAIN_BUFFER.print(&format!("Discord: listening thread had error - {}", err));
                continue;
            }
        };
        state.state.update(&event);
        if let discord::model::Event::MessageCreate(message) = event {
            // TODO: message.mention_roles
            let is_self = is_self_mentioned(&state,
                                            &message.channel_id,
                                            message.mention_everyone,
                                            Some(message.mentions),
                                            None);
            display(&state,
                    &message.content,
                    &message.channel_id,
                    Some(message.author),
                    is_self);
            MAIN_BUFFER.print(&message.content);
        }
    }
}

fn get_buffer(state: &ConnectionState, channel_id: &ChannelId) -> Option<Buffer> {
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

fn is_self_mentioned(state: &ConnectionState,
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

fn display(state: &ConnectionState,
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
