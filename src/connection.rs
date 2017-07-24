use std::sync::mpsc::*;
use std::thread::{spawn, JoinHandle};
use discord;
use discord::*;
use discord::model::*;

use command_print;
use ffi::*;
use message;
use event_proc;
use types::*;

pub struct ChannelData<'a> {
    pub state: &'a State,
    pub discord: &'a Discord,
    pub channel: ChannelRef<'a>,
    pub buffer: Buffer,
}

impl<'dis> ChannelData<'dis> {
    pub fn sync(&self) {
        self.buffer
            .set("localvar_set_channelid",
                 &format!("{}", self.channel.id().0));
        self.buffer
            .set("short_name", &self.channel.name(&NameFormat::prefix()));
        self.buffer.set("title", "Channel Title");
        self.buffer.set("type", "formatted");
        // Undocumented localvar found by digging through source.
        // Causes indentation on channels.
        if let ChannelRef::Public(_, _) = self.channel {
            self.buffer.set("localvar_set_type", "channel");
            self.buffer.set("nicklist", "1");
        } else {
            self.buffer.set("localvar_set_type", "private");
        }
        // TODO: buffer.set("localvar_set_type", "server");
        // Also undocumented, causes [nick] prefix.
        self.buffer
            .set("localvar_set_nick", &self.state.user().username);
        if let ChannelRef::Public(ref server, _) = self.channel {
            // TODO: This is suuuuper slow
            for member in &server.members {
                let name = member.name(&NameFormat::none());
                if !self.buffer.nick_exists(&name) {
                    self.buffer.add_nick(&name);
                }
            }
        }
    }

    fn from_buffer_impl(state: &'dis State, buffer: &Buffer) -> Option<ChannelRef<'dis>> {
        let channel_id_str = tryopt!(buffer.get("localvar_channelid"));
        let channel_id = ChannelId(tryopt!(channel_id_str.parse().ok()));
        state.find_channel(channel_id)
    }

    fn from_buffer(state: &'dis State,
                   discord: &'dis Discord,
                   buffer: Buffer)
                   -> ::std::result::Result<ChannelData<'dis>, Buffer> {
        match Self::from_buffer_impl(state, &buffer) {
            Some(channel) => {
                Ok(ChannelData {
                       state: state,
                       discord: discord,
                       channel: channel,
                       buffer: buffer,
                   })
            }
            None => Err(buffer),
        }
    }

    pub fn from_channel(state: &'dis State,
                        discord: &'dis Discord,
                        channel: ChannelRef<'dis>,
                        auto_open: bool)
                        -> Option<ChannelData<'dis>> {
        let (server_id, channel_id) = match channel {
            ChannelRef::Private(ref private) => (ServerId(0), private.id()),
            ChannelRef::Group(ref group) => (ServerId(0), group.id()),
            ChannelRef::Public(ref server, ref channel) => (server.id(), channel.id()),
        };
        let name_id = format!("{}.{}", server_id, channel_id);
        let (buffer, is_new) = if let Some(buffer) = Buffer::search(&name_id) {
            (buffer, false)
        } else if auto_open {
            (Buffer::new(&name_id, buffer_input).unwrap(), true)
        } else {
            return None;
        };
        let result = ChannelData {
            state: state,
            discord: discord,
            channel: channel,
            buffer: buffer,
        };
        if is_new {
            result.sync();
        }
        Some(result)
    }

    pub fn from_discord_event(state: &'dis State,
                              discord: &'dis Discord,
                              channel_id: ChannelId)
                              -> Option<ChannelData<'dis>> {
        let channel_ref = tryopt!(state.find_channel(channel_id));
        let is_private = if let ChannelRef::Public(_, _) = channel_ref {
            false
        } else {
            true
        };
        Self::from_channel(state, discord, channel_ref, is_private)
    }

    pub fn create_server(server: &LiveServer) {
        let name_id = format!("{}", server.id());
        let buffer = if let Some(buffer) = Buffer::search(&name_id) {
            buffer
        } else {
            Buffer::new(&name_id, |_, _| {}).unwrap()
        };
        buffer.set("short_name", &server.name(&NameFormat::prefix()));
        // TODO: Unify?
    }
}

fn buffer_input(buffer: Buffer, message: &str) {
    let (state, discord) = match MyConnection::magic() {
        Some(con) => (&con.state, &con.discord),
        None => {
            buffer.print("Discord is not connected");
            return;
        }
    };
    let channel = ChannelData::from_buffer(state, discord, buffer);
    let channel = match channel {
        Ok(x) => x,
        Err(buffer) => {
            buffer.print("Associated channel not found!?");
            return;
        }
    };
    let to_send = message::format_message_send(&channel.channel, message.into());
    let result = channel
        .discord
        .send_message(channel.channel.id(), &to_send, "", false);
    match result {
        Ok(_) => (),
        Err(err) => channel.buffer.print(&format!("{}", err)),
    }
}

pub fn debug_command(command: &str) {
    if let Some(x) = MyConnection::magic() {
        x.debug_command(command)
    }
}

pub fn query_command(buffer: &Buffer, user: &str) {
    if let Some(x) = MyConnection::magic() {
        x.query_command(buffer, user)
    }
}

pub struct MyConnection {
    state: State,
    discord: Discord,
    recv: Receiver<discord::Result<Event>>,
    _poke_fd: PokeableFd,
    _listen_thread: JoinHandle<()>,
}

static mut MAGIC: *mut MyConnection = 0 as *mut _;

impl MyConnection {
    pub fn magic() -> Option<&'static mut MyConnection> {
        unsafe {
            if MAGIC.is_null() {
                None
            } else {
                Some(&mut *MAGIC)
            }
        }
    }

    pub fn create(token: String) {
        if unsafe { MAGIC.is_null() } {
            let con = match MyConnection::new(token) {
                Ok(con) => Box::into_raw(Box::new(con)),
                Err(err) => {
                    MAIN_BUFFER.print("Error connecting:");
                    MAIN_BUFFER.print(&format!("{}", err));
                    return;
                }
            };
            unsafe {
                MAGIC = con;
            }
        }
    }

    pub fn drop() {
        unsafe {
            if !MAGIC.is_null() {
                let _ = Box::from_raw(MAGIC);
                MAGIC = 0 as *mut _;
            }
        }
    }

    fn debug_command(&mut self, command: &str) {
        if command == "replace" {
            for server in self.state.servers() {
                MAIN_BUFFER.print(&format!("Server: {}", &server.name));
                if let Some(chan) = self.state.find_channel(server.channels[0].id) {
                    for (user, mention) in message::all_names(&chan, &NameFormat::prefix()) {
                        MAIN_BUFFER.print(&format!("{} : {}", user, mention))
                    }
                }
            }
        }
    }

    fn query_command(&mut self, buffer: &Buffer, nick: &str) {
        if let Some(user) = message::all_names_everywhere(&self.state,
                                                          |name, user| if name == nick {
                                                              Some(user.id())
                                                          } else {
                                                              None
                                                          }) {
            for existing in self.state.private_channels() {
                if existing.recipient.id() == user {
                    ChannelData::from_channel(&self.state,
                                              &self.discord,
                                              ChannelRef::Private(existing),
                                              true);
                    return;
                }
            }
            match self.discord.create_private_channel(user) {
                Ok(new_channel) => {
                    ChannelData::from_channel(&self.state,
                                              &self.discord,
                                              ChannelRef::Private(&new_channel),
                                              true);
                }
                Err(err) => {
                    buffer.print(&format!("Unable to create a PM with {}: {}", user, err));
                }
            }
        } else {
            buffer.print(&format!("User not found: {}", nick));
        }
    }

    fn on_poke(&mut self) {
        loop {
            let event = self.recv.try_recv();
            let event = match event {
                Ok(Ok(event)) => event,
                Ok(Err(err)) => {
                    command_print(&format!("listening thread had error - {}", err));
                    continue;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    command_print("Listening thread stopped!");
                    break;
                }
            };
            self.state.update(&event);
            event_proc::on_event(&self.state, &self.discord, event);
        }
    }

    fn run_thread(mut connection: Connection,
                  pipe_poker: PokeableFdPoker,
                  send: Sender<discord::Result<Event>>) {
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
    }

    fn new(token: String) -> discord::Result<MyConnection> {
        let discord = Discord::from_user_token(&token)?;
        let (connection, ready) = discord.connect()?;
        let state = State::new(ready);
        connection.sync_servers(&state.all_servers()[..]);
        let (send, recv) = channel();
        let pipe = PokeableFd::new(move || if let Some(x) = Self::magic() {
                                       x.on_poke()
                                   });
        let pipe_poker = pipe.get_poker();
        let listen_thread = spawn(move || Self::run_thread(connection, pipe_poker, send));
        event_proc::open_and_sync_buffers(&state, &discord);
        // let completion_hook =
        // ffi::hook_completion("weecord_completion", "",
        // move |buffer, completion| {
        //     if let Some(state) = state_comp.upgrade() {
        //         do_completion(&*state.borrow(), buffer, completion)
        //     };
        // });
        Ok(MyConnection {
               discord: discord,
               state: state,
               recv: recv,
               _poke_fd: pipe,
               _listen_thread: listen_thread,
           })
    }
}
