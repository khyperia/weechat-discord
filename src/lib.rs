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
use discord::{Discord, State, ChannelRef, GetMessages};
use discord::model::{Event, ChannelType, User, LiveServer, PossibleServer, OnlineStatus,
                     Attachment};
use discord::model::{UserId, RoleId, ServerId, ChannelId, MessageId};
use ffi::{Buffer, MAIN_BUFFER, Hook, Completion, PokeableFd, WeechatObject};
use types::{Name, Id, DiscordId, NameFormat};
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
    _completion_hook: Hook,
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
    static DO_COMP_STATIC: fn(&'static mut ConnectionState, ffi::Buffer, ffi::Completion) =
        do_completion;
    let hook = match ffi::hook_completion("weecord_completion", "", &DO_COMP_STATIC) {
        Some(hook) => hook,
        None => {
            MAIN_BUFFER.print("Error: failed to hook completion");
            return;
        }
    };
    command_print("connecting");
    let discord = match Discord::new(&email, &password) {
        Ok(discord) => discord,
        Err(err) => {
            command_print(&format!("Login error: {}", err));
            return;
        }
    };
    let (mut connection, ready) = match discord.connect() {
        Ok(ok) => ok,
        Err(err) => {
            command_print(&format!("connection error: {}", err));
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
    let mut state = ConnectionState {
        discord: discord,
        state: dis_state,
        events: recv,
        _pipe: pipe,
        _completion_hook: hook,
    };
    process_event(&mut state, &Event::Ready(ready_clone));
    connection.sync_servers(&state.state.all_servers()[..]);
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
         channel_id: ChannelId,
         message: &str) {
    let state = match state {
        Some(state) => state,
        None => return,
    };
    let message = replace_mentions_send(&state.state, channel_id, message);
    let result = state.discord.send_message(&channel_id, &message, "", false);
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

fn process_event(state: &mut ConnectionState, event: &Event) {
    //MAIN_BUFFER.print(&format!("{:?}", event));
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
                    let buffer = get_buffer(state, channel.id());
                    if let Some(buffer) = buffer {
                        for member in &server.members {
                            if let Some(presence) = server.find_presence(member.id()) {
                                if presence.status == OnlineStatus::Online ||
                                   presence.status == OnlineStatus::Idle {
                                    buffer.add_nick(&member.user.name(&NameFormat::none()));
                                }
                            }
                        }
                    }
                }
            }
        }
        Event::Resumed { .. } => {}
        Event::UserUpdate(_) => {}
        Event::UserNoteUpdate(_, _) => {}
        Event::UserSettingsUpdate { .. } => {}
        Event::UserServerSettingsUpdate(_) => {}
        Event::MessageCreate(ref message) => {
            let is_self = is_self_mentioned(&state,
                                            &message.channel_id,
                                            message.mention_everyone,
                                            Some(&message.mentions),
                                            Some(&message.mention_roles));
            display(&state,
                    message.channel_id,
                    message.id,
                    Some(&message.author),
                    Some(&message.content),
                    Some(&message.attachments),
                    "",
                    is_self,
                    false)
        }
        Event::MessageUpdate { id,
                               channel_id,
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
                    channel_id,
                    id,
                    author.as_ref(),
                    content.as_ref().map(|x| &**x),
                    attachments.as_ref(),
                    "EDIT: ",
                    is_self,
                    false)
        }
        Event::MessageDelete { message_id, channel_id } => {
            display(&state,
                    channel_id,
                    message_id,
                    None,
                    None,
                    None,
                    "DELETE: ",
                    false,
                    false);
        }
        Event::ServerCreate(PossibleServer::Online(ref server)) => {
            for channel in &server.channels {
                let _ = get_buffer(state, channel.id());
            }
        }
        Event::ServerCreate(PossibleServer::Offline(_)) => (),
        Event::ServerMemberAdd(server_id, ref member) => {
            buffer_nicklist_update_id(state, server_id, &member.user, true)
        }
        Event::ServerMemberUpdate { server_id, ref user, .. } => {
            buffer_nicklist_update_id(state, server_id, user, true)
        }
        Event::ServerMemberRemove(server_id, ref user) => {
            buffer_nicklist_update_id(state, server_id, &user, false)
        }
        Event::ServerMembersChunk(server_id, ref members) => {
            for member in members {
                buffer_nicklist_update_id(state, server_id, &member.user, true);
            }
        }
        Event::ServerSync { server_id, ref members, .. } => {
            for member in members {
                buffer_nicklist_update_id(state, server_id, &member.user, true);
            }
        }
        Event::ChannelCreate(ref channel) => {
            get_buffer(state, channel.id());
        }
        Event::ChannelUpdate(ref channel) => {
            get_buffer(state, channel.id());
        }
        Event::ChannelDelete(ref channel) => {
            get_buffer(state, channel.id());
        }
        Event::PresenceUpdate { ref presence, .. } => {
            for ref server in state.state.servers() {
                if let Some(user) = server.find_user(presence.user_id) {
                    // let is_adding = presence.status == OnlineStatus::Online || presence.status == OnlineStatus::Idle;
                    let is_adding = true;
                    buffer_nicklist_update(state, server, user, is_adding);
                }
            }
        }
        Event::VoiceStateUpdate(_, _) => {}
        Event::VoiceServerUpdate { .. } => {}
        Event::CallCreate(_) => {}
        Event::CallUpdate { .. } => {}
        Event::CallDelete(_) => {}
        Event::ChannelRecipientAdd(_, _) => {}
        Event::ChannelRecipientRemove(_, _) => {}
        Event::TypingStart { .. } => {}
        Event::PresencesReplace(_) => {}
        Event::RelationshipAdd(_) => {}
        Event::RelationshipRemove(_, _) => {}
        Event::MessageAck { .. } => {}
        Event::MessageDeleteBulk { .. } => {}
        Event::ServerUpdate(_) => {}
        Event::ServerDelete(_) => {}
        Event::ServerRoleCreate(_, _) => {}
        Event::ServerRoleUpdate(_, _) => {}
        Event::ServerRoleDelete(_, _) => {}
        Event::ServerBanAdd(_, _) => {}
        Event::ServerBanRemove(_, _) => {}
        Event::ServerIntegrationsUpdate(_) => {}
        Event::ServerEmojisUpdate(_, _) => {}
        Event::ChannelPinsAck { .. } => {}
        Event::ChannelPinsUpdate { .. } => {}
        Event::Unknown(_, _) => {}
        _ => (),
    }
}

fn buffer_nicklist_update_id(state: &ConnectionState,
                             server_id: ServerId,
                             user: &User,
                             is_adding: bool) {
    for ref server in state.state.servers() {
        if server.id() == server_id {
            buffer_nicklist_update(state, server, user, is_adding);
        }
    }
}

fn buffer_nicklist_update(state: &ConnectionState,
                          server: &LiveServer,
                          user: &User,
                          is_adding: bool) {
    for channel in &server.channels {
        if let Some(buffer) = get_buffer(state, channel.id()) {
            if is_adding {
                buffer.add_nick(&user.name(&NameFormat::none()));
            } else {
                buffer.remove_nick(&user.name(&NameFormat::none()));
            }
        }
    }
}

fn do_completion(state: &mut ConnectionState, buffer: Buffer, mut completion: Completion) {
    let _ = buffer;
    for server in state.state.servers() {
        for member in server.members.iter() {
            let name = member.name(&NameFormat::prefix());
            completion.add(&name);
        }
    }
}

impl Buffer {
    fn load_backlog(&self, state: &ConnectionState, channel_id: ChannelId) {
        let messages = state.discord.get_messages(channel_id, GetMessages::MostRecent, None);
        match messages {
            Ok(messages) => {
                for message in messages.iter().rev() {
                    display(&state,
                            message.channel_id,
                            message.id,
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

fn get_buffer(state: &ConnectionState, channel_id: ChannelId) -> Option<Buffer> {
    let channel = try_opt!(state.state.find_channel(&channel_id));
    let server = if let ChannelRef::Public(srv, ch) = channel {
        if ch.kind != ChannelType::Text {
            return None;
        }
        Some(srv)
    } else {
        None
    };
    let channel_name = channel.name(&NameFormat::prefix());
    let channel_id = channel.id();
    let server_name = server.map(|s| s.name(&NameFormat::none()));
    let server_id = server.map_or(ServerId(0), |s| s.id());
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
            buffer.load_backlog(state, channel_id);
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
    let server = state.state.find_channel(&channel_id).and_then(|channel| match channel {
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

fn find_name<'a, SearchId: DiscordId, T: 'a + Name<SelfId = SearchId>, I: Iterator<Item = &'a T>>
    (items: I,
     id: SearchId,
     format: &NameFormat)
     -> Option<String> {
    items.into_iter()
        .find(|ref item| item.id() == id)
        .map(|ref item| item.name(format))
}

fn all_names<'a>(chan_ref: &ChannelRef<'a>) -> Vec<User> {
    match *chan_ref {
        ChannelRef::Private(ref private) => vec![private.recipient.clone()],
        ChannelRef::Group(ref group) => group.recipients.clone(),
        ChannelRef::Public(ref public, _) => {
            public.members.iter().map(|m| m.user.clone()).collect()
        }
    }
}

fn replace_mentions_send(state: &State, channel_id: ChannelId, mut content: String) -> String {
    let channel = match state.find_channel(&channel_id) {
        Some(channel) => channel,
        None => return content,
    };
    let names = all_names(&channel).into_iter()
        .map(|user| (format!("@{}", user.name(&NameFormat::none())), user.mention()))
        .collect::<Vec<_>>();
    // sort by descending length order
    names.sort_by(|(a, _), (b, _)| b.len().cmp(a.len()));
    for (name, mention) in names.iter() {
        if content.contains(name.0) {
            content = content.replace(name.0, format!("{}", name.1));
        }
    }
    content
}

fn replace_mentions(state: &State, channel_id: ChannelId, content: &str) -> String {
    lazy_static! {
        static ref RE: Regex = Regex::new(r"<(?P<type>@|@!|@&|#)(?P<id>\d+)>").unwrap();
    }
    let channel = match state.find_channel(&channel_id) {
        Some(ch) => ch,
        None => return content.into(),
    };
    let format = NameFormat::color_prefix();
    RE.replace_all(content, |ref captures: &regex::Captures| {
        let mention_type = unwrap!(captures.name("type"));
        let id = unwrap!(captures.name("id"));
        id.parse::<u64>()
            .ok()
            .and_then(|id| {
                match channel {
                    ChannelRef::Private(ref private) => {
                        if private.recipient.id() == UserId(id) {
                            Some(private.recipient.name(&format))
                        } else if state.user().id() == UserId(id) {
                            Some(state.user().name(&format))
                        } else {
                            None
                        }
                    }
                    ChannelRef::Public(ref server, _) => {
                        match mention_type {
                            "@" => {
                                find_name(server.members.iter().map(|x| &x.user),
                                          UserId(id),
                                          &format)
                            }
                            "@!" => find_name(server.members.iter(), UserId(id), &format),
                            "@&" => find_name(server.roles.iter(), RoleId(id), &format),
                            "#" => find_name(server.channels.iter(), ChannelId(id), &format),
                            _ => None,
                        }
                    }
                    ChannelRef::Group(ref group) => {
                        match mention_type {
                            "@" | "@!" => find_name(group.recipients.iter(), UserId(id), &format),
                            _ => None,
                        }
                    }
                }
            })
            .unwrap_or(captures.at(0).expect("Regex had no capture group 0").into())
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
           channel_id: ChannelId,
           message_id: MessageId,
           author: Option<&User>,
           content: Option<&str>,
           attachments: Option<&Vec<Attachment>>,
           prefix: &'static str,
           self_mentioned: bool,
           no_highlight: bool) {
    let channel = state.state.find_channel(&channel_id);
    if let Some(ChannelRef::Public(server, _)) = channel {
        if let Some(author) = author {
            buffer_nicklist_update(state, server, author, true);
        }
    }
    let buffer = match get_buffer(state, channel_id) {
        Some(buffer) => buffer,
        None => return,
    };
    let author_format = NameFormat::color();
    let (author, content, no_highlight): (String, String, bool) = match (author, content) {
        (Some(author), Some(content)) => {
            (author.name(&author_format),
             content.into(),
             no_highlight || author.id() == state.state.user().id())
        }
        (Some(author), None) => {
            match find_old_msg(&buffer, &message_id) {
                Some((_, content)) => {
                    (author.name(&author_format),
                     content,
                     no_highlight || author.id == state.state.user().id)
                }
                None => {
                    (author.name(&author_format),
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
    } else if let Some(ChannelRef::Private(_)) = channel {
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
