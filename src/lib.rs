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

use ffi::*;
use connection::*;

pub use ffi::wdr_init;
pub use ffi::wdr_end;

mod weechat {
    pub const COMMAND: &'static str = "discord";
    pub const DESCRIPTION: &'static str = "\
Discord from the comfort of your favorite command-line IRC client!
Source code available at https://github.com/khyperia/weechat-discord

How does channel muting work?
If plugins.var.weecord.mute.<channel_id> is set to the literal \"1\", \
then that buffer will not be opened. When a Discord channel is muted \
(in the official client), weechat-discord detects this and automatically \
sets this setting for you. If you would like to override this behavior \
and un-mute the channel, set the setting to \"0\". (Do not unset it, as it \
will just get automatically filled in again)

Options used:

plugins.var.weecord.token = <discord_token>
plugins.var.weecord.rename.<id> = <string>
plugins.var.weecord.mute.<channel_id> = (0|1)
plugins.var.weecord.on_delete.<server_id> = <channel_id>
";
    pub const ARGS: &'static str = "\
                     connect
                     disconnect
                     token <token>";
    pub const ARGDESC: &'static str = "\
connect: sign in to discord and open chat buffers
disconnect: sign out of Discord
token: set Discord login token
query: open PM buffer with user

Example:
  /discord token 123456789ABCDEF
  /discord connect
  /discord query khyperia
  /discord disconnect
";
    pub const COMPLETIONS: &'static str = "\
connect || disconnect || token || debug replace || query";
}

// *DO NOT* touch this outside of init/end
static mut MAIN_COMMAND_HOOK: *mut HookCommand = 0 as *mut _;

// Called when plugin is loaded in Weechat
pub fn init() -> Option<()> {
    let hook = tryopt!(ffi::hook_command(weechat::COMMAND,
                                         weechat::DESCRIPTION,
                                         weechat::ARGS,
                                         weechat::ARGDESC,
                                         weechat::COMPLETIONS,
                                         move |buffer, input| run_command(&buffer, input)));
    unsafe {
        MAIN_COMMAND_HOOK = Box::into_raw(Box::new(hook));
    };
    Some(())
}

// Called when plugin is unloaded from Weechat
pub fn end() -> Option<()> {
    unsafe {
        let _ = Box::from_raw(MAIN_COMMAND_HOOK);
        MAIN_COMMAND_HOOK = ::std::ptr::null_mut();
    };
    Some(())
}

fn user_set_option(name: &str, value: &str) {
    command_print(&ffi::set_option(name, value));
}

fn command_print(message: &str) {
    MAIN_BUFFER.print(&format!("{}: {}", &weechat::COMMAND, message));
}

fn run_command(buffer: &Buffer, command: &str) {
    // TODO: Add rename command
    if command == "" {
        command_print("see /help discord for more information")
    } else if command == "connect" {
        match ffi::get_option("token") {
            Some(t) => MyConnection::create(t),
            None => {
                command_print("Error: plugins.var.weecord.token unset. Run:");
                command_print("/discord token 123456789ABCDEF");
                return;
            }
        };
    } else if command == "disconnect" {
        MyConnection::drop();
        command_print("disconnected");
    } else if command.starts_with("token ") {
        let token = &command["token ".len()..];
        user_set_option("token", token.trim_matches('"'));
    } else if command.starts_with("query ") {
        query_command(buffer, &command["debug ".len()..]);
    } else if command.starts_with("debug ") {
        debug_command(&command["debug ".len()..]);
    } else {
        command_print("unknown command");
    }
}
