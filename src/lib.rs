extern crate discord;
extern crate libc;
extern crate regex;
#[macro_use]
extern crate lazy_static;

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
use discord::model::{Event, Channel, ChannelType, ChannelId, ServerId, RoleId, User,
                     PossibleServer, Member, PublicChannel, Role, CurrentUser};
use ffi::{Buffer, MAIN_BUFFER, PokeableFd};
use regex::Regex;

mod weechat {
    pub const COMMAND: &'static str = "discord";
    pub const DESCRIPTION: &'static str = "\
Discord from the comfort of your favorite command-line IRC client!
This plugin is a work in progress and could use your help.
Check it out at https://github.com/khyperia/weechat-discord";
    pub const ARGS: &'static str = "\
                     connect
                     disconnect
                     email <email>
                     password <password>";
    pub const ARGDESC: &'static str = "\
   connect: sign in to discord and open chat buffers
disconnect: sign out of Discord and close chat buffers
     email: set Discord login email
  password: set Discord login password

Example:
  /discord email your.email@example.com
  /discord password yourpassword
  /discord connect

";
    pub const COMPLETIONS: &'static str = "connect || disconnect || email || password";
}

pub struct ConnectionState {
    discord: Discord,
    state: State,
    events: Receiver<discord::Result<Event>>,
    _pipe: PokeableFd,
}

// Called when plugin is loaded in Weechat
pub fn init() {
    ffi::hook_command(weechat::COMMAND,
                      weechat::DESCRIPTION,
                      weechat::ARGS,
                      weechat::ARGDESC,
                      weechat::COMPLETIONS);
}

// Called when plugin is unloaded in Weechat
#[allow(unused)]
pub fn end(state: &Option<ConnectionState>) {}

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
    let ready_clone = ready.clone();
    let dis_state = State::new(ready);

    // TODO: on_ready (open buffers, etc)
    buffer.print("Discord: Connected");
    let (send, recv) = channel();
    let pipe = PokeableFd::new(Box::new(process_events));
    let pipe_poker = pipe.get_poker();
    let state = ConnectionState {
        discord: discord,
        state: dis_state,
        events: recv,
        _pipe: pipe,
    };
    process_event(&state, &Event::Ready(ready_clone));
    ffi::set_global_state(state);
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
    let state = match state {
        Some(state) => state,
        None => return,
    };
    let result = state.discord.send_message(channel_id, message, "", false);
    match result {
        Ok(_) => (),
        Err(err) => buffer.print(&format!("Discord: error sending message - {}", err)),
    };
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
            Ok(event) => event,
            Err(err) => {
                MAIN_BUFFER.print(&format!("Discord: listening thread had error - {}", err));
                continue;
            }
        };
        state.state.update(&event);
        process_event(state, &event);
    }
}

fn process_event(state: &ConnectionState, event: &Event) {
    match *event {
        Event::Ready(ref ready) => {
            for private in &ready.private_channels {
                let _ = get_buffer(state, &private.id);
            }
            for server in &ready.servers {
                let server = match *server {
                    PossibleServer::Online(ref server) => server,
                    PossibleServer::Offline(_) => continue,
                };
                for channel in &server.channels {
                    let _ = get_buffer(state, &channel.id);
                }
            }
        }
        Event::MessageCreate(ref message) => {
            let is_self = is_self_mentioned(&state,
                                            &message.channel_id,
                                            message.mention_everyone,
                                            Some(&message.mentions),
                                            Some(&message.mention_roles));
            display(&state,
                    &message.content,
                    &message.channel_id,
                    Some(&message.author),
                    is_self)
        }
        Event::MessageUpdate { ref channel_id,
                               ref content,
                               ref author,
                               ref mention_everyone,
                               ref mentions,
                               ref mention_roles,
                               .. } => {
            let is_self = is_self_mentioned(&state,
                                            &channel_id,
                                            mention_everyone.unwrap_or(false),
                                            mentions.as_ref(),
                                            mention_roles.as_ref());
            display(&state,
                    content.as_ref().map(|x| &**x).unwrap_or("<no content>"),
                    &channel_id,
                    author.as_ref(),
                    is_self)
        }
        Event::MessageDelete { ref channel_id, .. } => {
            display(&state, "[deleted a message]", &channel_id, None, false);
        }
        Event::ServerCreate(PossibleServer::Online(ref server)) => {
            for channel in &server.channels {
                let _ = get_buffer(state, &channel.id);
            }
        }
        Event::ServerCreate(PossibleServer::Offline(_)) => (),
        Event::ServerMemberAdd(_, _) => (),
        Event::ServerMemberUpdate { .. } => (),
        Event::ServerMemberRemove(_, _) => (),
        Event::ServerMembersChunk(_, _) => (),
        Event::ChannelCreate(ref channel) => {
            get_buffer(state, chan_id(&channel));
        }
        Event::ChannelUpdate(ref channel) => {
            get_buffer(state, chan_id(&channel));
        }
        Event::ChannelDelete(ref channel) => {
            get_buffer(state, chan_id(&channel));
        }
        _ => (),
    }
    fn chan_id(channel: &Channel) -> &ChannelId {
        match *channel {
            Channel::Private(ref ch) => &ch.id,
            Channel::Public(ref ch) => &ch.id,
        }
    }
}

fn get_buffer(state: &ConnectionState, channel_id: &ChannelId) -> Option<Buffer> {
    let (server_name, channel_name, server_id, channel_id) = {
        let channel = try_opt!(state.state.find_channel(channel_id));
        let channel = match channel {
            ChannelRef::Private(ch) => {
                Some(("discord-pm".into(), ch.recipient.name.clone(), ServerId(0), ch.id))
            }
            ChannelRef::Public(_, ch) if ch.kind != ChannelType::Text => None,
            ChannelRef::Public(srv, ch) => {
                Some((srv.name.clone(), format!("#{}", ch.name), srv.id, ch.id))
            }
        };
        try_opt!(channel)
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
                     mentions: Option<&Vec<User>>,
                     roles: Option<&Vec<RoleId>>)
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

fn format_mention(name: &str) -> String {
    let surround = if let Some(color) = ffi::info_get("nick_color", name) {
        Some((color, "\u{1c}"))
    } else {
        None
    };
    match surround {
        Some((l, r)) => format!("{}@{}{}", l, name, r),
        None => format!("@{}", name)
    }
}

fn format_channel(channel: &PublicChannel) -> String {
    format!("#{}", channel.name)
}

fn format_mention_user(user: &User) -> String {
    format_mention(&user.name)
}

fn format_mention_current_user(user: &CurrentUser) -> String {
    format_mention(&user.username)
}

fn format_mention_member(member: &Member) -> String {
    if let Some(ref nick) = member.nick {
        format_mention(nick)
    } else {
        format_mention_user(&member.user)
    }
}

fn format_role(role: &Role) -> String {
    format_mention(&role.name)
}

fn replace_mentions(state: &State, channel_id: &ChannelId, content: &str) -> String {
    lazy_static! {
        static ref RE: Regex = Regex::new(r"<(?P<type>@|@!|@&|#)(?P<id>\d+)>").unwrap();
    }
    RE.replace_all(content, |ref captures: &regex::Captures| {
        let ty = captures.name("type").unwrap();
        captures.name("id")
            .and_then(|id| id.parse::<u64>().ok())
            .and_then(|id| {
                match state.find_channel(channel_id) {
                    Some(ChannelRef::Private(ref private)) => {
                        if private.recipient.id.0 == id {
                            Some(format_mention_user(&private.recipient))
                        } else if state.user().id.0 == id {
                            Some(format_mention_current_user(&state.user()))
                        } else {
                            None
                        }
                    }
                    Some(ChannelRef::Public(ref server, _)) if ty == "@" => {
                        server.members
                            .iter()
                            .find(|ref member| member.user.id.0 == id)
                            .map(|ref member| format_mention_user(&member.user))
                    }
                    Some(ChannelRef::Public(ref server, _)) if ty == "@!" => {
                        server.members
                            .iter()
                            .find(|ref member| member.user.id.0 == id)
                            .map(|ref member| format_mention_member(&member))
                    }
                    Some(ChannelRef::Public(ref server, _)) if ty == "@&" => {
                        server.roles
                            .iter()
                            .find(|ref role| role.id.0 == id)
                            .map(|ref role| format_role(&role))
                    }
                    Some(ChannelRef::Public(ref server, _)) if ty == "#" => {
                        server.channels
                            .iter()
                            .find(|ref channel| channel.id.0 == id)
                            .map(|ref channel| format_channel(&channel))
                    }
                    _ => None,
                }
            })
            .unwrap_or(format_mention("[unknown]"))
    })
}

fn display(state: &ConnectionState,
           content: &str,
           channel_id: &ChannelId,
           author: Option<&User>,
           self_mentioned: bool) {
    let buffer = match get_buffer(state, channel_id) {
        Some(buffer) => buffer,
        None => return,
    };

    let mut tags = Vec::new();
    if self_mentioned {
        tags.push("notify_highlight".into());
    } else {
        tags.push("notify_message".into());
    };
    let name = author.map_or("[unknown]".into(), |x| format_mention_user(x));
    tags.push(format!("nick_{}", name));
    buffer.print_tags(&tags.join(",".into()),
                      &format!("{}\t{}",
                               name,
                               replace_mentions(&state.state, channel_id, content)));
}
