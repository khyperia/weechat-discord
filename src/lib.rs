extern crate discord;
extern crate regex;
#[macro_use]
extern crate lazy_static;

#[macro_use]
mod macros;
pub mod ffi;
mod types;
mod util;

use std::mem::drop;
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::error::Error;
use std::iter::IntoIterator;
use std::thread::spawn;
use discord::{Discord, State, ChannelRef};
use discord::model::{Event, Channel, ChannelType, ChannelId, ServerId, RoleId, MessageId, User,
                     PossibleServer, OnlineStatus, Attachment};
use ffi::{Buffer, MAIN_BUFFER, PokeableFd, WeechatObject};
use types::{Mention, Name, Id, DiscordId};
use util::ServerExt;
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

fn user_set_option(name: &str, value: &str) {
    command_print(&ffi::set_option(name, value));
}

fn connect() {
    let (email, password) = match (ffi::get_option("email"), ffi::get_option("password")) {
        (Some(e), Some(p)) => (e, p),
        (email, password) => {
            MAIN_BUFFER.print("Error: plugins.var.weecord.{email,password} unset. Run:");
            if email.is_none() {
                MAIN_BUFFER.print("/discord email your.email@example.com");
            }
            if password.is_none() {
                MAIN_BUFFER.print("/discord password hunter2");
            }
            return;
        }
    };
    command_print("connecting");
    let discord = match Discord::new(&email, &password) {
        Ok(discord) => discord,
        Err(err) => {
            command_print(&format!("connection error: {}", err.description()));
            return;
        }
    };
    let (mut connection, ready) = match discord.connect() {
        Ok(ok) => ok,
        Err(err) => {
            command_print(&format!("connection error: {}", err.description()));
            return;
        }
    };
    let ready_clone = ready.clone();
    let dis_state = State::new(ready);

    // TODO: on_ready (open MAIN_BUFFERs, etc)
    command_print("connected");
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

fn command_print(message: &str) {
    MAIN_BUFFER.print(&format!("{}: {}", &weechat::COMMAND, message));
}

fn run_command(buffer: Buffer, state: Option<&mut ConnectionState>, command: &str) -> bool {
    let _ = state;
    let _ = buffer;
    if command == "" {
        command_print("see /help discord for more information")
    } else if command == "connect" {
        connect();
    } else if command == "disconnect" {
        command_print("disconnected");
        return false;
    } else if command.starts_with("email ") {
        user_set_option("email", &command["email ".len()..]);
    } else if command.starts_with("password ") {
        user_set_option("password", &command["password ".len()..]);
    } else {
        command_print("unknown command");
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
                command_print("Listening thread stopped!");
                return;
            }
        };
        let event = match event {
            Ok(event) => event,
            Err(err) => {
                command_print(&format!("listening thread had error - {}", err));
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
            // TODO: Setting for auto-opening private buffers
            // for private in &ready.private_channels {
            //    let _ = get_buffer(state, &private.id);
            // }
            for server in &ready.servers {
                let server = match *server {
                    PossibleServer::Online(ref server) => server,
                    PossibleServer::Offline(_) => continue,
                };
                for channel in &server.channels {
                    let buffer = get_buffer(state, &channel.id);
                    if let Some(buffer) = buffer {
                        for member in &server.members {
                            if let Some(presence) = server.find_presence(member.user.id) {
                                if presence.status == OnlineStatus::Online ||
                                   presence.status == OnlineStatus::Idle {
                                    buffer.add_nick(&member.user.name());
                                }
                            }
                        }
                    }
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
                    &message.channel_id,
                    &message.id,
                    Some(&message.author),
                    Some(&message.content),
                    Some(&message.attachments),
                    "",
                    is_self,
                    false)
        }
        Event::MessageUpdate { ref id,
                               ref channel_id,
                               ref content,
                               ref author,
                               ref mention_everyone,
                               ref mentions,
                               ref mention_roles,
                               ref attachments,
                               .. } => {
            let is_self = is_self_mentioned(&state,
                                            &channel_id,
                                            mention_everyone.unwrap_or(false),
                                            mentions.as_ref(),
                                            mention_roles.as_ref());
            display(&state,
                    &channel_id,
                    &id,
                    author.as_ref(),
                    content.as_ref().map(|x| &**x),
                    attachments.as_ref(),
                    "EDIT: ",
                    is_self,
                    false)
        }
        Event::MessageDelete { ref message_id, ref channel_id } => {
            display(&state,
                    &channel_id,
                    &message_id,
                    None,
                    None,
                    None,
                    "DELETE: ",
                    false,
                    false);
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
        Event::PresenceUpdate { ref presence, .. } => {
            for ref server in state.state.servers() {
                if let Some(user) = server.find_user(presence.user_id) {
                    for channel in &server.channels {
                        let buffer = get_buffer(state, &channel.id);
                        if let Some(buffer) = buffer {
                            if presence.status == OnlineStatus::Online ||
                               presence.status == OnlineStatus::Idle {
                                buffer.add_nick(&user.name());
                            } else {
                                buffer.remove_nick(&user.name());
                            }
                        }
                    }
                }
            }
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

impl Buffer {
    fn load_backlog(&self, state: &ConnectionState, channel_id: &ChannelId) {
        let last_id = state.state.find_channel(channel_id).and_then(|ch| match ch {
            ChannelRef::Private(ch) => ch.last_message_id,
            ChannelRef::Public(_, ch) => ch.last_message_id,
        });
        let messages = state.discord.get_messages(channel_id, last_id.as_ref(), None, None);
        match messages {
            Ok(messages) => {
                for message in messages.iter().rev() {
                    display(&state,
                            &message.channel_id,
                            &message.id,
                            Some(&message.author),
                            Some(&message.content),
                            Some(&message.attachments),
                            "",
                            false,
                            true)
                }
            }
            Err(err) => {
                self.print(&format!("Failed to load backlog (loading from disk instead): {}",
                                    err.description()));
                self.load_weechat_backlog();
            }
        }
    }
}

fn get_buffer(state: &ConnectionState, channel_id: &ChannelId) -> Option<Buffer> {
    let (server_name, channel_name, server_id, channel_id) = {
        let channel = try_opt!(state.state.find_channel(channel_id));
        let channel = match channel {
            ChannelRef::Private(ch) => {
                Some((None, ch.recipient.name().clone(), ServerId(0), ch.id))
            }
            ChannelRef::Public(_, ch) if ch.kind != ChannelType::Text => None,
            ChannelRef::Public(srv, ch) => {
                Some((Some(srv.name().clone()), ch.mention(), srv.id, ch.id))
            }
        };
        try_opt!(channel)
    };
    let buffer_id = format!("{}.{}", server_id.0, channel_id.0);
    let buffer_name = if let Some(server_name) = server_name {
        format!("{} {}", server_name, channel_name)
    } else {
        channel_name
    };
    let buffer = match Buffer::search(&buffer_id) {
        Some(buffer) => buffer,
        None => {
            let buffer = try_opt!(Buffer::new(&buffer_id, &channel_id));
            buffer.set("short_name", &buffer_name);
            buffer.set("title", "Channel Title");
            buffer.set("type", "formatted");
            buffer.set("nicklist", "1");
            buffer.load_backlog(state, &channel_id);
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

fn format_mention(name: &str, include_at: bool) -> String {
    let surround = if let Some(color) = ffi::info_get("nick_color", name) {
        Some((color, "\u{1c}"))
    } else {
        None
    };
    let at = if include_at {
        "@"
    } else {
        ""
    };
    match surround {
        Some((l, r)) => format!("{}{}{}{}", l, at, name, r),
        None => format!("{}{}", at, name),
    }
}

fn find_mention<'a, T: 'a + Mention + Id, I: Iterator<Item = &'a T>>(mentionables: I,
                                                                     id: DiscordId)
                                                                     -> Option<String> {
    mentionables.into_iter()
        .find(|ref mention| mention.id() == id)
        .map(|ref mention| mention.mention())
}

fn replace_mentions(state: &State, channel_id: &ChannelId, content: &str) -> String {
    lazy_static! {
        static ref RE: Regex = Regex::new(r"<(?P<type>@|@!|@&|#)(?P<id>\d+)>").unwrap();
    }
    RE.replace_all(content, |ref captures: &regex::Captures| {
        let x = unwrap!(captures.name("type"));
        captures.name("id")
            .and_then(|id| id.parse::<DiscordId>().ok())
            .and_then(|id| {
                match state.find_channel(channel_id) {
                    Some(ChannelRef::Private(ref private)) => {
                        if private.recipient.id() == id {
                            Some(private.recipient.mention())
                        } else if state.user().id() == id {
                            Some(state.user().mention())
                        } else {
                            None
                        }
                    }
                    Some(ChannelRef::Public(ref server, _)) => {
                        match x {
                            "@" => find_mention(server.members.iter().map(|x| &x.user), id),
                            "@!" => find_mention(server.members.iter(), id),
                            "@&" => find_mention(server.roles.iter(), id),
                            "#" => find_mention(server.channels.iter(), id),
                            _ => None,
                        }
                    }
                    _ => None,
                }
            })
            .unwrap_or(format_mention("unknown", true))
    })
}

fn find_tag<T, F: Fn(String) -> Option<T>>(line_data: &ffi::WeechatAny, pred: F) -> Option<T> {
    let tagcount: i32 = unwrap!(line_data.get("tags_count"));
    let tagcount = tagcount as usize;
    for i in 0..tagcount {
        let tag: String = unwrap!(line_data.get_idx::<ffi::SharedString>("tags_array", i)).0;
        if let Some(result) = pred(tag) {
            return Some(result);
        }
    }
    None
}

// returns: (Prefix, Message)
fn find_old_msg(buffer: &Buffer, message_id: &MessageId) -> Option<(String, String)> {
    let searchterm = format!("discord_messageid_{}", message_id.0);
    let mut line = unwrap!(unwrap!(buffer.get_any("lines")).get_any("last_line"));
    for _ in 0..100 {
        let data = unwrap!(line.get_any("data"));
        if let Some(()) = find_tag(&data, |tag| if tag == searchterm {
            Some(())
        } else {
            None
        }) {
            let prefix = unwrap!(data.get::<ffi::SharedString>("prefix")).0;
            let message = unwrap!(data.get("message"));
            return Some((prefix, message));
        }
        if let Some(prev) = line.get_any("prev_line") {
            line = prev;
        } else {
            break;
        }
    }
    None
}

fn display(state: &ConnectionState,
           channel_id: &ChannelId,
           message_id: &MessageId,
           author: Option<&User>,
           content: Option<&str>,
           attachments: Option<&Vec<Attachment>>,
           prefix: &'static str,
           self_mentioned: bool,
           no_highlight: bool) {
    let buffer = match get_buffer(state, channel_id) {
        Some(buffer) => buffer,
        None => return,
    };
    let (author, content, no_highlight) = match (author, content) {
        (Some(author), Some(content)) => {
            (format_mention(&author.name(), false),
             content.into(),
             no_highlight || author.id == state.state.user().id)
        }
        (Some(author), None) => {
            match find_old_msg(&buffer, &message_id) {
                Some((_, content)) => {
                    (format_mention(&author.name(), false),
                     content,
                     no_highlight || author.id == state.state.user().id)
                }
                None => {
                    (format_mention(&author.name(), false),
                     "<no content>".into(),
                     no_highlight || author.id == state.state.user().id)
                }
            }
        }
        (None, Some(content)) => {
            match find_old_msg(&buffer, &message_id) {
                Some((author, _)) => (author, content.into(), no_highlight),
                None => ("[unknown]".into(), content.into(), no_highlight),
            }
        }
        (None, None) => {
            match find_old_msg(&buffer, &message_id) {
                Some((author, content)) => (author, content, no_highlight),
                None => return, // don't bother, we have absolutely nothing
            }
        }
    };
    let mut tags = Vec::new();
    if no_highlight {
        tags.push("no_highlight".into());
        tags.push("notify_none".into());
    } else if self_mentioned {
        tags.push("notify_highlight".into());
    } else if let Some(ChannelRef::Private(_)) = state.state.find_channel(channel_id) {
        tags.push("notify_private".into());
    } else {
        tags.push("notify_message".into());
    };
    tags.push(format!("nick_{}", author));
    tags.push(format!("discord_messageid_{}", message_id.0));
    let tags = tags.join(",".into());
    let content = replace_mentions(&state.state, channel_id, &content);
    // first into_iter is the Option iterator
    let attachments = attachments.into_iter()
        .flat_map(|attachments| attachments.into_iter().map(|a| a.proxy_url.clone()));
    let content = if content.is_empty() {
        None
    } else {
        Some(content)
    };
    let content = content.into_iter().chain(attachments).collect::<Vec<_>>().join("\n");
    buffer.print_tags(&tags, &format!("{}\t{}{}", author, prefix, content));
}
