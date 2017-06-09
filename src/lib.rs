extern crate discord;
extern crate libc;

#[macro_use]
mod macros;
mod ffi;
mod types;
mod util;
mod connection;
mod message;
mod event_proc;

use discord::*;

use ffi::*;
use types::*;
use connection::*;

pub use ffi::wdr_init;
pub use ffi::wdr_end;

mod weechat {
    pub const COMMAND: &'static str = "discord";
    pub const DESCRIPTION: &'static str = "\
Discord from the comfort of your favorite command-line IRC client!
This plugin is a work in progress and could use your help.
Check it out at https://github.com/khyperia/weechat-discord

Options used:

plugins.var.weecord.token = <discord_token>
plugins.var.weecord.on_delete.<server_id> = <channel_id>
plugins.var.weecord.rename.<id> = <string>
";
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
    pub const COMPLETIONS: &'static str = "connect || disconnect || token || debug replace";
}

// Called when plugin is loaded in Weechat
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

fn user_set_option(name: &str, value: &str) {
    command_print(&ffi::set_option(name, value));
}

fn command_print(message: &str) {
    MAIN_BUFFER.print(&format!("{}: {}", &weechat::COMMAND, message));
}

fn debug_command(state: &State, command: &str) {
    if command == "replace" {
        for server in state.servers() {
            MAIN_BUFFER.print(&format!("Server: {}", &server.name));
            if let Some(chan) = state.find_channel(server.channels[0].id) {
                for (user, mention) in message::all_names(&chan, &NameFormat::prefix()) {
                    MAIN_BUFFER.print(&format!("{} : {}", user, mention))
                }
            }
        }
    }
}

fn run_command(buffer: Buffer, state: &mut Option<MyConnection>, command: &str) {
    let _ = buffer;
    if command == "" {
        command_print("see /help discord for more information")
    } else if command == "connect" {
        match ffi::get_option("token") {
            Some(t) => {
                match MyConnection::new(t) {
                    Ok(con) => *state = Some(con),
                    Err(err) => {
                        MAIN_BUFFER.print("Error connecting:");
                        MAIN_BUFFER.print(&format!("{}", err));
                    }
                }
            }
            None => {
                MAIN_BUFFER.print("Error: plugins.var.weecord.token unset. Run:");
                MAIN_BUFFER.print("/discord token 123456789ABCDEF");
                return;
            }
        };
    } else if command == "disconnect" {
        *state = None;
        command_print("disconnected");
    } else if command.starts_with("token ") {
        user_set_option("token", &command["token ".len()..]);
    } else if command.starts_with("debug ") {
        if let &mut Some(ref state) = state {
            debug_command(&state.state.read().unwrap(), &command["debug ".len()..]);
        }
    } else {
        command_print("unknown command");
    }
}
