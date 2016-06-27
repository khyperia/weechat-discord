use discord::ChannelRef;
use discord::model::{User, Member, PrivateChannel, PublicChannel,
                     Role, CurrentUser, LiveServer};
use ffi;
use format_mention;

pub type DiscordId = u64;

pub trait Id {
    fn id(&self) -> DiscordId;
}

pub trait Name: Id {
    fn name(&self) -> String;
}

pub trait Mention: Name {
    fn mention(&self) -> String;
}

fn get_rename_option(id: DiscordId) -> Option<String> {
    let option = format!("rename.{}", id);
    ffi::get_option(&option)
}

impl Id for User {
    fn id(&self) -> DiscordId {
        self.id.0
    }
}

impl Name for User {
    fn name(&self) -> String {
        get_rename_option(self.id()).unwrap_or(self.name.clone())
    }
}

impl Mention for User {
    fn mention(&self) -> String {
        format_mention(&self.name())
    }
}

impl Id for Member {
    fn id(&self) -> DiscordId {
        self.user.id()
    }
}

impl Name for Member {
    fn name(&self) -> String {
        get_rename_option(self.id())
            .unwrap_or(
                if let Some(ref nick) = self.nick {
                    nick.clone()
                } else {
                    self.user.name()
                }
            )
    }
}

impl Mention for Member {
    fn mention(&self) -> String {
        format_mention(&self.name())
    }
}

impl Id for CurrentUser {
    fn id(&self) -> DiscordId {
        self.id.0
    }
}

impl Name for CurrentUser {
    fn name(&self) -> String {
        get_rename_option(self.id()).unwrap_or(self.username.clone())
    }
}

impl Mention for CurrentUser {
    fn mention(&self) -> String {
        format_mention(&self.username)
    }
}

impl<'a> Id for ChannelRef<'a> {
    fn id(&self) -> DiscordId {
        match *self {
            ChannelRef::Public(_, ref chan) => chan.id(),
            ChannelRef::Private(ref chan) => chan.id(),
        }
    }
}

impl<'a> Name for ChannelRef<'a> {
    fn name(&self) -> String {
        get_rename_option(self.id())
            .unwrap_or(
                match *self {
                    ChannelRef::Public(_, ref chan) => chan.name.clone(),
                    ChannelRef::Private(ref chan) => chan.recipient.name.clone(),
                }
            )
    }
}

impl<'a> Mention for ChannelRef<'a> {
    fn mention(&self) -> String {
        match *self {
            ChannelRef::Public(_, ref chan) => chan.mention(),
            ChannelRef::Private(ref chan) => chan.mention(),
        }
    }
}

impl Id for PublicChannel {
    fn id(&self) -> DiscordId {
        self.id.0
    }
}

impl Name for PublicChannel {
    fn name(&self) -> String {
        get_rename_option(self.id()).unwrap_or(self.name.clone())
    }
}

impl Mention for PublicChannel {
    fn mention(&self) -> String {
        format!("#{}", self.name)
    }
}

impl Id for PrivateChannel {
    fn id(&self) -> DiscordId {
        self.id.0
    }
}

impl Name for PrivateChannel {
    fn name(&self) -> String {
        get_rename_option(self.id()).unwrap_or(self.recipient.name())
    }
}

impl Mention for PrivateChannel {
    fn mention(&self) -> String {
        format!("@{}", self.recipient.name)
    }
}

impl Id for Role {
    fn id(&self) -> DiscordId {
        self.id.0
    }
}

impl Name for Role {
    fn name(&self) -> String {
        get_rename_option(self.id()).unwrap_or(self.name.clone())
    }
}

impl Mention for Role {
    fn mention(&self) -> String {
        format_mention(&self.name)
    }
}

impl Id for LiveServer {
    fn id(&self) -> DiscordId {
        self.id.0
    }
}

impl Name for LiveServer {
    fn name(&self) -> String {
        get_rename_option(self.id()).unwrap_or(self.name.clone())
    }
}
