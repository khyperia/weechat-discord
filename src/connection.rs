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
    discord: Rc<Discord>,
}

pub fn buffer_name(channel: ChannelRef) -> (String, String) {
    let server = if let ChannelRef::Public(srv, _) = channel {
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
}

pub struct ChannelData {
    state: RcState,
    sender: OutgoingPipe,
    id: ChannelId,
}

impl ChannelData {
    pub fn create(state: &RcState, sender: &OutgoingPipe, id: ChannelId) -> Option<Buffer> {
        let locked_state = state.read().unwrap();
        let channel = match locked_state.find_channel(id) {
            Some(ch) => ch,
            None => return None,
        };
        let (name_id, name_short) = buffer_name(channel);
        if let Some(buffer) = Buffer::search(&name_id) {
            return Some(buffer);
        }
        let me = ChannelData {
            state: state.clone(),
            sender: sender.clone(),
            id: id,
        };
        let me = Box::new(me);
        Buffer::new(&name_id, me).map(|buffer| {
            buffer.set("short_name", &name_short);
            buffer.set("title", "Channel Title");
            buffer.set("type", "formatted");
            buffer.set("nicklist", "1");
            // TODO
            // buffer.load_backlog();
            buffer
        })
    }
}

impl BufferImpl for ChannelData {
    fn input(&self, buffer: Buffer, message: &str) {
        let to_send = message::format_message_send(&self.state, self.id, message);
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
