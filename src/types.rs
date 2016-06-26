use discord::ChannelRef;
use discord::model::{User, Member, PrivateChannel, PublicChannel, Role, CurrentUser};
use ::format_mention;

pub trait Mention {
    fn mention(&self) -> String;
}

impl Mention for User {
    fn mention(&self) -> String {
        format_mention(&self.name)
    }
}

impl Mention for CurrentUser {
    fn mention(&self) -> String {
        format_mention(&self.username)
    }
}

impl Mention for Member {
    fn mention(&self) -> String {
        if let Some(ref nick) = self.nick {
            format_mention(nick)
        } else {
            self.user.mention()
        }
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

impl Mention for PublicChannel {
    fn mention(&self) -> String {
        format!("#{}", self.name)
    }
}

impl Mention for PrivateChannel {
    fn mention(&self) -> String {
        format!("@{}", self.recipient.name)
    }
}

impl Mention for Role {
    fn mention(&self) -> String {
        format_mention(&self.name)
    }
}

pub type Id = u64;

pub trait DiscordId {
    fn id(&self) -> Id;
}

impl DiscordId for User {
    fn id(&self) -> Id {
        self.id.0
    }
}

impl DiscordId for CurrentUser {
    fn id(&self) -> Id {
        self.id.0
    }
}

impl DiscordId for Member {
    fn id(&self) -> Id {
        self.user.id()
    }
}

impl DiscordId for Role {
    fn id(&self) -> Id {
        self.id.0
    }
}

impl<'a> DiscordId for ChannelRef<'a> {
    fn id(&self) -> Id {
        match *self {
            ChannelRef::Public(_, ref chan) => chan.id(),
            ChannelRef::Private(ref chan) => chan.id(),
        }
    }
}

impl DiscordId for PublicChannel {
    fn id(&self) -> Id {
        self.id.0
    }
}

impl DiscordId for PrivateChannel {
    fn id(&self) -> Id {
        self.id.0
    }
}
