use discord::*;
use discord::model::*;

use types::*;
use ffi;
use ffi::*;
use connection::*;

pub struct FormattedMessage {
    pub channel: String,
    pub author: String,
    pub prefix: &'static str,
    pub content: String,
    pub tags: String,
}

impl FormattedMessage {
    pub fn print(&self, target: &Buffer) {
        target.print_tags(&self.tags,
                          &format!("{}\t{}{}", self.author, self.prefix, self.content))
    }
}

pub fn is_self_mentioned(channel: &ChannelData,
                         mention_everyone: bool,
                         author: Option<&User>,
                         mentions: Option<&Vec<User>>,
                         roles: Option<&Vec<RoleId>>)
                         -> bool {
    let me = channel.state.user();
    if author.map(|a| a.id()) == Some(me.id()) {
        return false;
    }
    if mention_everyone {
        return true;
    }
    if let Some(mentions) = mentions {
        for mention in mentions {
            if me.id == mention.id {
                return true;
            }
        }
    }
    let server = match channel.channel {
        ChannelRef::Public(ref server, _) => server,
        _ => return false,
    };
    let roles = if let Some(roles) = roles {
        roles
    } else {
        return false;
    };
    for member in &server.members {
        if member.user.id == me.id {
            for member_role in &member.roles {
                for role in roles {
                    if member_role.0 == role.0 {
                        return true;
                    }
                }
            }
            break;
        }
    }
    false
}

pub fn all_names_everywhere<T, F: FnMut(String, &User) -> Option<T>>(state: &State,
                                                                     mut f: F)
                                                                     -> Option<T> {
    let format = NameFormat::none();
    for server in state.servers() {
        for member in &server.members {
            if let Some(x) = f(member.user.name(&format), &member.user) {
                return Some(x);
            }
            if let Some(x) = f(member.name(&format), &member.user) {
                return Some(x);
            }
        }
    }
    for group in state.groups().values() {
        for recipient in &group.recipients {
            if let Some(x) = f(recipient.name(&format), &recipient) {
                return Some(x);
            }
        }
    }
    for private in state.private_channels() {
        if let Some(x) = f(private.recipient.name(&format), &private.recipient) {
            return Some(x);
        }
    }
    return None;
}

pub fn all_names(chan_ref: &ChannelRef, format: &NameFormat) -> Vec<(String, String)> {
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

fn replace_mentions_send(channel: &ChannelRef, mut content: String) -> String {
    for (name, mention) in all_names(channel, &NameFormat::prefix()) {
        if content.contains(&*name) {
            content = content.replace(&*name, &format!("{}", mention));
        }
    }
    content
}

fn replace_mentions(channel: &ChannelRef, mut content: String) -> String {
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
fn find_old_msg(buffer: &Buffer, message_id: MessageId) -> Option<(String, String)> {
    let searchterm = format!("discord_messageid_{}", message_id.0);
    let mut result = None;
    if let Some(mut line) = unwrap!(buffer.get_any("lines")).get_any("first_line") {
        loop {
            let data = unwrap!(line.get_any("data"));
            let found_tag = find_tag(&data, |tag| if tag == searchterm { Some(()) } else { None });
            if let Some(()) = found_tag {
                let prefix = unwrap!(data.get::<ffi::SharedString>("prefix")).0;
                let message = unwrap!(data.get("message"));
                result = Some(match result {
                    Some((prefix, previous)) => (prefix, format!("{}\n{}", previous, message)),
                    None => (prefix, message),
                });
            }
            if let Some(next) = line.get_any("next_line") {
                line = next;
            } else {
                break;
            }
        }
    }
    result
}

pub fn resolve_message(author: Option<&User>,
                       content: Option<&str>,
                       buffer: &Buffer,
                       channel_ref: &ChannelRef,
                       message_id: MessageId)
                       -> Option<(String, String)> {
    let author_format = NameFormat::color();
    if let (Some(author), Some(content)) = (author, content) {
        let content = replace_mentions(channel_ref, content.into());
        // Check for member-defined name instead of user name
        if let &ChannelRef::Public(ref server, _) = channel_ref {
            if let Some(member) = server.members.iter().find(|m| m.id() == author.id()) {
                return Some((member.name(&author_format), content.into()));
            }
        }
        Some((author.name(&author_format), content.into()))
    } else {
        find_old_msg(buffer, message_id)
    }
}

pub fn format_message(channel: &ChannelData,
                      message_id: MessageId,
                      author: Option<&User>,
                      content: Option<&str>,
                      attachments: Option<&Vec<Attachment>>,
                      prefix: &'static str,
                      self_mentioned: bool)
                      -> Option<FormattedMessage> {
    let is_private = if let ChannelRef::Public(_, _) = channel.channel {
        false
    } else {
        true
    };
    let (author, content) = tryopt!(resolve_message(author,
                                                    content,
                                                    &channel.buffer,
                                                    &channel.channel,
                                                    message_id));
    let tags = {
        let mut tags = Vec::new();
        if self_mentioned {
            tags.push("notify_highlight".into());
        } else if is_private {
            tags.push("notify_private".into());
        } else {
            tags.push("notify_message".into());
        };
        tags.push(format!("nick_{}", author));
        tags.push(format!("discord_messageid_{}", message_id.0));
        tags.join(",".into())
    };
    let content = {
        let mut content_list = Vec::new();
        if !content.is_empty() {
            content_list.push(content);
        }
        if let Some(attachments) = attachments {
            for attachment in attachments {
                content_list.push(attachment.proxy_url.clone());
            }
        }
        content_list.join("\n")
    };
    Some(FormattedMessage {
             channel: channel.channel.name(&NameFormat::none()),
             author: author,
             prefix: prefix,
             content: content,
             tags: tags,
         })
}

pub fn format_message_send(channel_ref: &ChannelRef, message: String) -> String {
    replace_mentions_send(channel_ref, message)
}
