extern crate discord;

#[macro_use]
mod macros;
pub mod ffi;
mod types;
mod util;

use std::borrow::Cow;
use std::cell::RefCell;
use std::error::Error;
use std::iter::IntoIterator;
use std::mem::drop;
use std::rc::Rc;
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::thread::spawn;

use discord::model::{Event, ChannelType, User, LiveServer, PossibleServer, Attachment, RoleId,
                     ServerId, ChannelId, MessageId};
use discord::{Discord, State, ChannelRef, GetMessages};

use ffi::{Buffer, MAIN_BUFFER, Hook, Completion, PokeableFd, WeechatObject};
use types::{Name, Id, NameFormat};
use util::ServerExt;

mod weechat {
    pub const COMMAND: &'static str = "discord";
    pub const DESCRIPTION: &'static str = "\
Discord from the comfort of your favorite command-line IRC client!
This plugin is a work in progress and could use your help.
Check it out at https://github.com/khyperia/weechat-discord";
    pub const ARGS: &'static str = "\
                     connect
                     disconnect
                     token <token>";
    pub const ARGDESC: &'static str = "\
   connect: sign in to discord and open chat buffers
disconnect: sign out of Discord and close chat buffers
     token: set Discord login token

Example:
  /discord token 123456789ABCDEF
  /discord connect

";
    pub const COMPLETIONS: &'static str = "connect || disconnect || token";
}

pub struct ConnectionState {
    discord: Discord,
    state: State,
    events: Receiver<discord::Result<Event>>,
    _completion_hook: Option<Hook>,
    _poke_fd: Option<PokeableFd>,
}

// Called when plugin is loaded in Weechat
// TODO
pub fn init() {
    let mut state = Box::new(None);
    let hook = ffi::hook_command(weechat::COMMAND,
                                 weechat::DESCRIPTION,
                                 weechat::ARGS,
                                 weechat::ARGDESC,
                                 weechat::COMPLETIONS,
                                 move |buffer, input| run_command(buffer, state.as_mut(), input));
    ::std::mem::forget(hook); // TODO: Memory leak here.
}

// Called when plugin is unloaded in Weechat
#[allow(unused)]
pub fn end(state: &Option<ConnectionState>) {}

fn user_set_option(name: &str, value: &str) {
    command_print(&ffi::set_option(name, value));
}

fn connect() -> Option<Rc<RefCell<ConnectionState>>> {
    let token = match ffi::get_option("token") {
        Some(t) => t,
        _ => {
            MAIN_BUFFER.print("Error: plugins.var.weecord.token unset. Run:");
            MAIN_BUFFER.print("/discord token 123456789ABCDEF");
            return None;
        }
    };
    command_print("connecting");
    let discord = match Discord::from_user_token(&token) {
        Ok(discord) => discord,
        Err(err) => {
            command_print(&format!("Login error: {}", err));
            return None;
        }
    };
    let (mut connection, ready) = match discord.connect() {
        Ok(ok) => ok,
        Err(err) => {
            command_print(&format!("connection error: {}", err));
            return None;
        }
    };
    let ready_clone = ready.clone();
    let dis_state = State::new(ready);
    // TODO: on_ready (open MAIN_BUFFERs, etc)
    command_print("connected");
    let (send, recv) = channel();
    let state = ConnectionState {
        discord: discord,
        state: dis_state,
        events: recv,
        _completion_hook: None,
        _poke_fd: None,
    };
    let state = Rc::new(RefCell::new(state));

    let state_comp = Rc::downgrade(&state);
    let completion_hook =
        ffi::hook_completion("weecord_completion", "", move |buffer, completion| {
            if let Some(state) = state_comp.upgrade() {
                do_completion(&*state.borrow(), buffer, completion)
            };
        });
    let completion_hook = match completion_hook {
        Some(hook) => hook,
        None => {
            MAIN_BUFFER.print("Error: failed to hook completion");
            return None;
        }
    };

    let state_pipe = Rc::downgrade(&state);
    let pipe = PokeableFd::new(move || if let Some(mut state) = state_pipe.upgrade() {
                                   process_events(&mut state);
                               });
    let pipe_poker = pipe.get_poker();

    {
        let state = &mut *state.borrow_mut();
        state._completion_hook = Some(completion_hook);
        state._poke_fd = Some(pipe);
    }

    process_event(&state, &Event::Ready(ready_clone));
    connection.sync_servers(&state.borrow().state.all_servers()[..]);
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
    Some(state)
}

fn command_print(message: &str) {
    MAIN_BUFFER.print(&format!("{}: {}", &weechat::COMMAND, message));
}

fn debug_command(state: &ConnectionState, command: &str) {
    if command == "replace" {
        for server in state.state.servers() {
            MAIN_BUFFER.print(&format!("Server: {}", &server.name));
            if let Some(chan) = state.state.find_channel(server.channels[0].id) {
                for (user, mention) in all_names(&chan, &NameFormat::prefix()) {
                    MAIN_BUFFER.print(&format!("{} : {}", user, mention))
                }
            }
        }
    }
}

fn run_command(buffer: Buffer, state: &mut Option<Rc<RefCell<ConnectionState>>>, command: &str) {
    let _ = buffer;
    if command == "" {
        command_print("see /help discord for more information")
    } else if command == "connect" {
        *state = connect();
    } else if command == "disconnect" {
        *state = None;
        command_print("disconnected");
    } else if command.starts_with("token ") {
        user_set_option("token", &command["token ".len()..]);
    } else if command.starts_with("debug ") {
        if let &mut Some(ref state) = state {
            debug_command(&state.borrow(), &command["debug ".len()..]);
        }
    } else {
        command_print("unknown command");
    }
}

fn input(state: &ConnectionState, buffer: Buffer, channel_id: ChannelId, message: &str) {
    let message = replace_mentions_send(&state.state, channel_id, message.into());
    let result = state
        .discord
        .send_message(channel_id, &message, "", false);
    match result {
        Ok(_) => (),
        Err(err) => buffer.print(&format!("Discord: error sending message - {}", err)),
    };
}

fn process_events(state: &Rc<RefCell<ConnectionState>>) {
    loop {
        let event = state.borrow().events.try_recv();
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
        process_event(state, &event);
    }
}

fn process_event(state: &Rc<RefCell<ConnectionState>>, event: &Event) {
    state.borrow_mut().state.update(event);
    // MAIN_BUFFER.print(&format!("{:?}", event));
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
                            buffer.add_nick(&member.user.name(&NameFormat::none()));
                        }
                    }
                }
            }
        }
        Event::MessageCreate(ref message) => {
            let is_self = is_self_mentioned(&*state.borrow(),
                                            message.channel_id,
                                            message.mention_everyone,
                                            Some(&message.mentions),
                                            Some(&message.mention_roles));
            display(state,
                    message.channel_id,
                    message.id,
                    Some(&message.author),
                    Some(&message.content),
                    Some(&message.attachments),
                    "",
                    is_self,
                    false)
        }
        Event::MessageUpdate {
            id,
            channel_id,
            ref content,
            ref author,
            ref mention_everyone,
            ref mentions,
            ref mention_roles,
            ref attachments,
            ..
        } => {
            let is_self = is_self_mentioned(&*state.borrow(),
                                            channel_id,
                                            mention_everyone.unwrap_or(false),
                                            mentions.as_ref(),
                                            mention_roles.as_ref());
            display(state,
                    channel_id,
                    id,
                    author.as_ref(),
                    content.as_ref().map(|x| &**x),
                    attachments.as_ref(),
                    "EDIT: ",
                    is_self,
                    false)
        }
        Event::MessageDelete {
            message_id,
            channel_id,
        } => {
            display(state,
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
        Event::ServerMemberAdd(server_id, ref member) => {
            buffer_nicklist_update_id(state, server_id, &member.user, true)
        }
        Event::ServerMemberUpdate {
            server_id,
            ref user,
            ..
        } => buffer_nicklist_update_id(state, server_id, user, true),
        Event::ServerMemberRemove(server_id, ref user) => {
            buffer_nicklist_update_id(state, server_id, user, false)
        }
        Event::ServerMembersChunk(server_id, ref members) |
        Event::ServerSync {
            server_id,
            ref members,
            ..
        } => {
            for member in members {
                buffer_nicklist_update_id(state, server_id, &member.user, true);
            }
        }
        Event::ChannelCreate(ref channel) |
        Event::ChannelUpdate(ref channel) |
        Event::ChannelDelete(ref channel) => {
            get_buffer(state, channel.id());
        }
        Event::PresenceUpdate { ref presence, .. } => {
            for server in state.borrow().state.servers() {
                if let Some(user) = server.find_user(presence.user_id) {
                    let is_adding = true;
                    buffer_nicklist_update(state, server, user, is_adding);
                }
            }
        }
        Event::Resumed { .. } |
        Event::UserUpdate(_) |
        Event::UserNoteUpdate(_, _) |
        Event::UserSettingsUpdate { .. } |
        Event::UserServerSettingsUpdate(_) |
        Event::VoiceStateUpdate(_, _) |
        Event::VoiceServerUpdate { .. } |
        Event::CallCreate(_) |
        Event::CallUpdate { .. } |
        Event::CallDelete(_) |
        Event::ChannelRecipientAdd(_, _) |
        Event::ChannelRecipientRemove(_, _) |
        Event::TypingStart { .. } |
        Event::PresencesReplace(_) |
        Event::RelationshipAdd(_) |
        Event::RelationshipRemove(_, _) |
        Event::MessageAck { .. } |
        Event::MessageDeleteBulk { .. } |
        Event::ServerCreate(PossibleServer::Offline(_)) |
        Event::ServerUpdate(_) |
        Event::ServerDelete(_) |
        Event::ServerRoleCreate(_, _) |
        Event::ServerRoleUpdate(_, _) |
        Event::ServerRoleDelete(_, _) |
        Event::ServerBanAdd(_, _) |
        Event::ServerBanRemove(_, _) |
        Event::ServerIntegrationsUpdate(_) |
        Event::ServerEmojisUpdate(_, _) |
        Event::ChannelPinsAck { .. } |
        Event::ChannelPinsUpdate { .. } |
        Event::Unknown(_, _) |
        _ => (),
    }
}

fn buffer_nicklist_update_id(state: &Rc<RefCell<ConnectionState>>,
                             server_id: ServerId,
                             user: &User,
                             is_adding: bool) {
    for server in state.borrow().state.servers() {
        if server.id() == server_id {
            buffer_nicklist_update(state, server, user, is_adding);
        }
    }
}

fn buffer_nicklist_update(state: &Rc<RefCell<ConnectionState>>,
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

fn do_completion(state: &ConnectionState, buffer: Buffer, mut completion: Completion) {
    let _ = buffer;
    for server in state.state.servers() {
        for member in &server.members {
            let name = member.name(&NameFormat::prefix());
            completion.add(&name);
        }
    }
}

impl Buffer {
    fn load_backlog(&self, state: &Rc<RefCell<ConnectionState>>, channel_id: ChannelId) {
        let messages = state
            .borrow()
            .discord
            .get_messages(channel_id, GetMessages::MostRecent, None);
        match messages {
            Ok(messages) => {
                for message in messages.iter().rev() {
                    display(state,
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

fn get_buffer(state: &Rc<RefCell<ConnectionState>>, channel_id: ChannelId) -> Option<Buffer> {
    let (buffer_id, buffer_name) = {
        let state = state.borrow();
        let channel = try_opt!(state.state.find_channel(channel_id));
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
        (buffer_id, buffer_name)
    };
    let buffer = match Buffer::search(&buffer_id) {
        Some(buffer) => buffer,
        None => {
            let state_weak = Rc::downgrade(state);
            let buffer =
                try_opt!(Buffer::new(&buffer_id, move |buffer, input_str| if let Some(state) =
                    state_weak.upgrade() {
                    input(&*state.borrow(), buffer, channel_id, input_str);
                }));
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
                     channel_id: ChannelId,
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
    let server = state
        .state
        .find_channel(channel_id)
        .and_then(|channel| match channel {
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
    };
    false
}

fn all_names(chan_ref: &ChannelRef, format: &NameFormat) -> Vec<(String, String)> {
    let mut names = Vec::new();
    match *chan_ref {
        ChannelRef::Private(private) => {
            names.push((private.recipient.name(format), format!("{}", private.recipient.mention())))
        }
        ChannelRef::Group(group) => {
            for recipient in &group.recipients {
                names.push((recipient.name(format), format!("{}", recipient.mention())))
            }
        }
        ChannelRef::Public(server, _) => {
            for member in &server.members {
                let mut mention = format!("{}", member.user.mention());
                // order of push matters (stable sort for nick/user names the same)
                names.push((member.user.name(format), mention.clone()));
                mention.insert(2, '!');
                names.push((member.name(format), mention));
            }
            for role in &server.roles {
                names.push((role.name(format), format!("{}", role.mention())));
            }
            for chan in &server.channels {
                names.push((chan.name(format), format!("{}", chan.mention())));
            }
        }
    }
    // sort by descending length order. Rust sort is stable sort.
    names.sort_by(|&(ref a, _), &(ref b, _)| b.len().cmp(&a.len()));
    names
}

fn replace_mentions_send<'a>(state: &State,
                             channel_id: ChannelId,
                             mut content: Cow<'a, str>)
                             -> Cow<'a, str> {
    let channel = match state.find_channel(channel_id) {
        Some(channel) => channel,
        None => return content,
    };
    for (name, mention) in all_names(&channel, &NameFormat::prefix()) {
        if content.contains(&*name) {
            content = content
                .into_owned()
                .replace(&*name, &format!("{}", mention))
                .into();
        }
    }
    content
}

fn replace_mentions(state: &State, channel_id: ChannelId, mut content: String) -> String {
    let channel = match state.find_channel(channel_id) {
        Some(ch) => ch,
        None => return content.into(),
    };
    for (name, mention) in all_names(&channel, &NameFormat::color_prefix()) {
        // check contains to reduce allocations
        if content.contains(&mention) {
            content = content.replace(&mention, &name);
        }
    }
    content
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

fn display(state: &Rc<RefCell<ConnectionState>>,
           channel_id: ChannelId,
           message_id: MessageId,
           author: Option<&User>,
           content: Option<&str>,
           attachments: Option<&Vec<Attachment>>,
           prefix: &'static str,
           self_mentioned: bool,
           no_highlight: bool) {
    let (is_private, temp) = {
        let state = state.borrow();
        let channel = state.state.find_channel(channel_id);
        let is_private = if let Some(ChannelRef::Private(_)) = channel {
            true
        } else {
            false
        };
        if let Some(ChannelRef::Public(server, _)) = channel {
            if let Some(author) = author {
                (is_private, Some((server.clone(), author.clone())))
            } else {
                (is_private, None)
            }
        } else {
            (is_private, None)
        }
    };
    if let Some((server, author)) = temp {
        buffer_nicklist_update(state, &server, &author, true);
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
             no_highlight || author.id() == state.borrow().state.user().id())
        }
        (Some(author), None) => {
            match find_old_msg(&buffer, &message_id) {
                Some((_, content)) => {
                    (author.name(&author_format),
                     content,
                     no_highlight || author.id == state.borrow().state.user().id)
                }
                None => {
                    (author.name(&author_format),
                     "<no content>".into(),
                     no_highlight || author.id == state.borrow().state.user().id)
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
    } else if is_private {
        tags.push("notify_private".into());
    } else {
        tags.push("notify_message".into());
    };
    tags.push(format!("nick_{}", author));
    tags.push(format!("discord_messageid_{}", message_id.0));
    let tags = tags.join(",".into());
    let content = replace_mentions(&state.borrow().state, channel_id, content);
    // first into_iter is the Option iterator
    let attachments =
        attachments
            .into_iter()
            .flat_map(|attachments| attachments.into_iter().map(|a| a.proxy_url.clone()));
    let content = if content.is_empty() {
        None
    } else {
        Some(content)
    };
    let content = content
        .into_iter()
        .chain(attachments)
        .collect::<Vec<_>>()
        .join("\n");
    buffer.print_tags(&tags, &format!("{}\t{}{}", author, prefix, content));
}
