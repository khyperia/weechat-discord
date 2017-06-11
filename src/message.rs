use discord::*;
use discord::model::*;

use types::*;
use ffi;
use ffi::*;
use connection::*;

pub struct FormattedMessage {
    pub target: Buffer,
    pub channel: String,
    pub author: String,
    pub prefix: &'static str,
    pub content: String,
    pub tags: String,
}

impl FormattedMessage {
    pub fn print(&self) {
        self.target
            .print_tags(&self.tags,
                        &format!("{}\t{}{}", self.author, self.prefix, self.content))
    }
}

pub fn is_self_mentioned(state: &State,
                         channel_id: ChannelId,
                         mention_everyone: bool,
                         mentions: Option<&Vec<User>>,
                         roles: Option<&Vec<RoleId>>)
                         -> bool {
    if mention_everyone {
        return true;
    }
    let me = state.user();
    if let Some(mentions) = mentions {
        for mention in mentions {
            if me.id == mention.id {
                return true;
            }
        }
    }
    let server = state
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

fn replace_mentions_send(state: &State, channel_id: ChannelId, mut content: String) -> String {
    let channel = match state.find_channel(channel_id) {
        Some(channel) => channel,
        None => return content,
    };
    for (name, mention) in all_names(&channel, &NameFormat::prefix()) {
        if content.contains(&*name) {
            content = content.replace(&*name, &format!("{}", mention));
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
    if let Some(mut line) = unwrap!(buffer.get_any("lines")).get_any("last_line") {
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
    }
    None
}

pub fn resolve_message(state: &State,
                       author: Option<&User>,
                       content: Option<&str>,
                       channel_id: ChannelId,
                       message_id: MessageId)
                       -> Option<(String, String)> {
    let author_format = NameFormat::color();
    if let (Some(author), Some(content)) = (author, content) {
        let content = replace_mentions(&state, channel_id, content.into());
        return Some((author.name(&author_format), content.into()));
    }
    let channel_ref = tryopt!(state.find_channel(channel_id));
    let buffer_id = buffer_name(channel_ref).0;
    let buffer = tryopt!(ffi::Buffer::search(&buffer_id));
    if let Some(value) = find_old_msg(&buffer, &message_id) {
        return Some(value);
    }
    return None;
}

pub fn format_message(state: &State,
                      channel_id: ChannelId,
                      message_id: MessageId,
                      author: Option<&User>,
                      content: Option<&str>,
                      attachments: Option<&Vec<Attachment>>,
                      prefix: &'static str,
                      self_mentioned: bool,
                      no_highlight: bool)
                      -> Option<FormattedMessage> {
    let channel_ref = tryopt!(state.find_channel(channel_id));
    let buffer_id = buffer_name(channel_ref).0;
    let buffer = tryopt!(ffi::Buffer::search(&buffer_id));
    let is_private = if let ChannelRef::Private(_) = channel_ref {
        true
    } else {
        false
    };
    let no_highlight = no_highlight || author.map(|a| a.id()) == Some(state.user().id());
    let (author, content) =
        tryopt!(resolve_message(state, author, content, channel_id, message_id));
    let tags = {
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
             target: buffer,
             channel: channel_ref.name(&NameFormat::none()),
             author: author,
             prefix: prefix,
             content: content,
             tags: tags,
         })
}

pub fn format_message_send(state: &RcState, channel_id: ChannelId, message: &str) -> String {
    replace_mentions_send(&state.read().unwrap(), channel_id, message.into())
}
