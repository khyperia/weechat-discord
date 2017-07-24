use std::borrow::Cow;
use std::cmp::Eq;
use discord::ChannelRef;
use discord::model::{User, Role, Emoji, Server, PrivateChannel, PublicChannel, Channel};
use discord::model::{UserId, RoleId, EmojiId, ServerId, ChannelId};
use discord::model::{Member, CurrentUser, LiveServer, Mention, Group};
use ffi;

fn get_rename_option<Id: DiscordId>(id: Id) -> Option<String> {
    ffi::get_option(&format!("rename.{}", id.raw_id()))
}

pub trait Mentionable {
    fn mention_tr(&self) -> Mention;
}

impl Mentionable for User {
    fn mention_tr(&self) -> Mention {
        self.mention()
    }
}
impl Mentionable for Member {
    fn mention_tr(&self) -> Mention {
        self.user.mention()
    }
}
impl Mentionable for Role {
    fn mention_tr(&self) -> Mention {
        self.mention()
    }
}
impl Mentionable for PublicChannel {
    fn mention_tr(&self) -> Mention {
        self.mention()
    }
}

pub trait DiscordId: Eq {
    fn raw_id(&self) -> u64;
}

impl DiscordId for UserId {
    fn raw_id(&self) -> u64 {
        self.0
    }
}

impl DiscordId for RoleId {
    fn raw_id(&self) -> u64 {
        self.0
    }
}

impl DiscordId for EmojiId {
    fn raw_id(&self) -> u64 {
        self.0
    }
}

impl DiscordId for ServerId {
    fn raw_id(&self) -> u64 {
        self.0
    }
}

impl DiscordId for ChannelId {
    fn raw_id(&self) -> u64 {
        self.0
    }
}

pub trait Id {
    type SelfId: DiscordId;
    fn id(&self) -> Self::SelfId;
}

impl Id for User {
    type SelfId = UserId;
    fn id(&self) -> Self::SelfId {
        self.id
    }
}

impl Id for CurrentUser {
    type SelfId = UserId;
    fn id(&self) -> Self::SelfId {
        self.id
    }
}

impl Id for Member {
    type SelfId = UserId;
    fn id(&self) -> Self::SelfId {
        self.user.id
    }
}

impl Id for Role {
    type SelfId = RoleId;
    fn id(&self) -> Self::SelfId {
        self.id
    }
}

impl Id for Emoji {
    type SelfId = EmojiId;
    fn id(&self) -> Self::SelfId {
        self.id
    }
}

impl Id for Server {
    type SelfId = ServerId;
    fn id(&self) -> Self::SelfId {
        self.id
    }
}

impl Id for LiveServer {
    type SelfId = ServerId;
    fn id(&self) -> Self::SelfId {
        self.id
    }
}

impl Id for PublicChannel {
    type SelfId = ChannelId;
    fn id(&self) -> Self::SelfId {
        self.id
    }
}

impl Id for PrivateChannel {
    type SelfId = ChannelId;
    fn id(&self) -> Self::SelfId {
        self.id
    }
}

impl Id for Group {
    type SelfId = ChannelId;
    fn id(&self) -> Self::SelfId {
        self.channel_id
    }
}

impl Id for Channel {
    type SelfId = ChannelId;
    fn id(&self) -> Self::SelfId {
        match *self {
            Channel::Private(ref ch) => ch.id(),
            Channel::Group(ref ch) => ch.id(),
            Channel::Public(ref ch) => ch.id(),
        }
    }
}

impl<'a> Id for ChannelRef<'a> {
    type SelfId = ChannelId;
    fn id(&self) -> Self::SelfId {
        match *self {
            ChannelRef::Public(_, chan) => chan.id(),
            ChannelRef::Group(group) => group.id(),
            ChannelRef::Private(chan) => chan.id(),
        }
    }
}

pub struct NameFormat {
    include_prefix: bool,
    include_color: bool,
}

// Helpers such that construction isn't as verbose
impl NameFormat {
    pub fn none() -> NameFormat {
        NameFormat {
            include_prefix: false,
            include_color: false,
        }
    }

    pub fn color() -> NameFormat {
        NameFormat {
            include_prefix: false,
            include_color: true,
        }
    }

    pub fn prefix() -> NameFormat {
        NameFormat {
            include_prefix: true,
            include_color: false,
        }
    }

    pub fn color_prefix() -> NameFormat {
        NameFormat {
            include_prefix: true,
            include_color: true,
        }
    }

    fn format(&self, prefix: &str, name: &str) -> String {
        let (left, right): (Cow<str>, &str) = if self.include_color {
                ffi::info_get("nick_color", name).map(|color| (color.into(), "\u{1c}"))
            } else {
                None
            }
            .unwrap_or(("".into(), ""));
        let at = if self.include_prefix { prefix } else { "" };
        format!("{}{}{}{}", left, at, name, right)
    }
}

pub trait Name: Id {
    // (prefix, raw_name)
    fn name_internal(&self) -> (&'static str, Cow<str>);
    fn name(&self, fmt: &NameFormat) -> String {
        let (prefix, raw_name) = self.name_internal();
        let rename = get_rename_option(self.id());
        let name: Cow<str> = rename.map_or(raw_name, |x| x.into());
        fmt.format(prefix, &name)
    }
}

impl Name for User {
    fn name_internal(&self) -> (&'static str, Cow<str>) {
        ("@", Cow::Borrowed(&*self.name))
    }
}

impl Name for Member {
    // self.nick or self.user.name()
    fn name_internal(&self) -> (&'static str, Cow<str>) {
        self.nick
            .as_ref()
            .map_or_else(|| self.user.name_internal(), |n| ("@", Cow::Borrowed(&**n)))
    }
}

impl Name for CurrentUser {
    fn name_internal(&self) -> (&'static str, Cow<str>) {
        ("@", Cow::Borrowed(&self.username))
    }
}

impl Name for PublicChannel {
    fn name_internal(&self) -> (&'static str, Cow<str>) {
        ("#", Cow::Borrowed(&self.name))
    }
}

impl Name for PrivateChannel {
    fn name_internal(&self) -> (&'static str, Cow<str>) {
        self.recipient.name_internal()
    }
}

impl Name for LiveServer {
    fn name_internal(&self) -> (&'static str, Cow<str>) {
        ("", Cow::Borrowed(&self.name))
    }
}

impl Name for Role {
    fn name_internal(&self) -> (&'static str, Cow<str>) {
        ("@", Cow::Borrowed(&self.name))
    }
}

static MAX_GROUP_LEN: usize = 16;

impl Name for Group {
    fn name_internal(&self) -> (&'static str, Cow<str>) {
        ("&", Cow::Owned(self.name().chars().take(MAX_GROUP_LEN).collect()))
    }
}

impl Name for Channel {
    fn name_internal(&self) -> (&'static str, Cow<str>) {
        match *self {
            Channel::Private(ref ch) => ch.name_internal(),
            Channel::Group(ref ch) => ch.name_internal(),
            Channel::Public(ref ch) => ch.name_internal(),
        }
    }
}

impl<'a> Name for ChannelRef<'a> {
    fn name_internal(&self) -> (&'static str, Cow<str>) {
        match *self {
            ChannelRef::Public(_, chan) => chan.name_internal(),
            ChannelRef::Group(chan) => chan.name_internal(),
            ChannelRef::Private(chan) => chan.name_internal(),
        }
    }
}
