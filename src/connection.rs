use std::sync::mpsc::*;
use std::sync::RwLock;
use std::rc::Rc;
use std::thread::{spawn, JoinHandle};
use discord;
use discord::*;
use discord::model::*;

use command_print;
use ffi::*;
use message;
use event_proc;
use types::*;

pub type RcState = Rc<RwLock<State>>;

#[derive(Clone)]
pub struct OutgoingPipe {
    pub discord: Rc<Discord>,
}

pub struct ChannelData {
    state: RcState,
    sender: OutgoingPipe,
    id: ChannelId,
}

struct ServerData {}

impl BufferImpl for ServerData {
    fn input(&self, buffer: Buffer, message: &str) {
        let _ = buffer;
        let _ = message;
    }

    fn close(&self, buffer: Buffer) {
        let _ = buffer;
    }
}

impl ChannelData {
    fn ensure_consistent(state: &RcState,
                         buffer: &Buffer,
                         server: ServerId,
                         channel: ChannelId,
                         name: &str) {
        buffer.set("short_name", name);
        buffer.set("title", "Channel Title");
        buffer.set("type", "formatted");
        buffer.set("nicklist", "1");
        // Undocumented localvar found by digging through source.
        // Causes indentation on channels.
        if channel.0 == 0 {
            buffer.set("localvar_set_type", "server");
        } else if server.0 == 0 {
            // Note: private causes an indent, which looks weird
            buffer.set("localvar_set_type", "private");
        } else {
            buffer.set("localvar_set_type", "channel");
        }
        // Also undocumented, causes [nick] prefix.
        buffer.set("localvar_set_nick", &state.read().unwrap().user().username);
    }

    pub fn create_server(state: &RcState, server: &LiveServer) {
        // This buffer is never used, it's just a buffer placeholder for formatting
        let name_id = format!("{}", server.id().0);
        let buffer = if let Some(buffer) = Buffer::search(&name_id) {
            buffer
        } else {
            Buffer::new(&name_id, Box::new(ServerData {})).unwrap()
        };
        let name_short = server.name(&NameFormat::prefix());
        Self::ensure_consistent(state, &buffer, server.id(), ChannelId(0), &name_short);
    }

    pub fn create(state: &RcState, sender: &OutgoingPipe, channel: &ChannelRef) -> Buffer {
        let (server_id, channel_id) = match channel {
            &ChannelRef::Private(ref private) => (ServerId(0), private.id()),
            &ChannelRef::Group(ref group) => (ServerId(0), group.id()),
            &ChannelRef::Public(ref server, ref channel) => (server.id(), channel.id()),
        };
        let name_id = format!("{}.{}", server_id, channel_id);
        let buffer = if let Some(buffer) = Buffer::search(&name_id) {
            buffer
        } else {
            let me = ChannelData {
                state: state.clone(),
                sender: sender.clone(),
                id: channel.id(),
            };
            let me = Box::new(me);
            Buffer::new(&name_id, me).unwrap()
            // TODO
            // buffer.load_backlog();
        };
        let name_short = channel.name(&NameFormat::prefix());
        Self::ensure_consistent(state, &buffer, server_id, channel_id, &name_short);
        buffer
    }
}

impl BufferImpl for ChannelData {
    fn input(&self, buffer: Buffer, message: &str) {
        let to_send = message::format_message_send(&self.state, self.id, message);
        let to_send = match to_send {
            Ok(x) => x,
            Err(err) => {
                buffer.print(&err);
                return;
            }
        };
        let result = self.sender
            .discord
            .send_message(self.id, &to_send, "", false);
        match result {
            Ok(_) => (),
            Err(err) => buffer.print(&format!("{}", err)),
        }
    }

    fn close(&self, buffer: Buffer) {
        let _ = buffer;
    }
}

pub struct MyConnection {
    pub state: RcState,
    _poke_fd: PokeableFd,
    _listean_thread: JoinHandle<()>,
}

impl MyConnection {
    pub fn new(token: String) -> discord::Result<MyConnection> {
        let discord = Discord::from_user_token(&token)?;
        let (mut connection, ready) = discord.connect()?;
        let state = Rc::new(RwLock::new(State::new(ready)));
        let (send, recv) = channel();
        let outgoing = OutgoingPipe { discord: Rc::new(discord) };
        event_proc::open_and_sync_buffers(&state, &outgoing);
        connection.sync_servers(&state.read().unwrap().all_servers()[..]);
        // let completion_hook =
        // ffi::hook_completion("weecord_completion", "",
        // move |buffer, completion| {
        //     if let Some(state) = state_comp.upgrade() {
        //         do_completion(&*state.borrow(), buffer, completion)
        //     };
        // });
        let pipe_state = state.clone();
        let pipe = PokeableFd::new(move || loop {
                                       let event = recv.try_recv();
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
                                       if let Ok(mut state) = pipe_state.write() {
                                           state.update(&event);
                                       } else {
                                           command_print("OH NO! State was already borrowed!");
                                       }
                                       event_proc::on_event(&pipe_state, &outgoing, event);
                                   });
        let pipe_poker = pipe.get_poker();
        let listen_thread = spawn(move || {
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
        Ok(MyConnection {
               state: state,
               _poke_fd: pipe,
               _listean_thread: listen_thread,
           })
    }
}
