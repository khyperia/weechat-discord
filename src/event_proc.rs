use discord::*;
use discord::model::*;

use ffi;
use connection::*;
use message::*;

pub fn open_and_sync_buffers(state: &State, discord: &Discord) {
    for server in state.servers() {
        ChannelData::create_server(server);
        for channel in &server.channels {
            if channel.kind == ChannelType::Voice {
                continue;
            }
            if let Some(x) = ChannelData::from_channel(state,
                                                       discord,
                                                       ChannelRef::Public(server, channel),
                                                       true) {
                x.sync()
            }
        }
    }
}

pub fn on_event(state: &State, discord: &Discord, event: &Event) -> Option<()> {
    match *event {
        Event::MessageCreate(ref message) => {
            let channel =
                tryopt!(ChannelData::from_discord_event(state, discord, message.channel_id));
            let is_self = is_self_mentioned(&channel,
                                            message.mention_everyone,
                                            Some(&message.author),
                                            Some(&message.mentions),
                                            Some(&message.mention_roles));
            let message = tryopt!(format_message(&channel,
                                                 message.id,
                                                 Some(&message.author),
                                                 Some(&message.content),
                                                 Some(&message.attachments),
                                                 "",
                                                 is_self));
            message.print(&channel.buffer);
        }
        Event::MessageUpdate {
            id,
            channel_id,
            ref content,
            ref author,
            ref mention_everyone,
            ref mentions,
            ref mention_roles,
            ref attachments,
            ..
        } => {
            let channel = tryopt!(ChannelData::from_discord_event(state, discord, channel_id));
            let is_self = is_self_mentioned(&channel,
                                            mention_everyone.unwrap_or(false),
                                            author.as_ref(),
                                            mentions.as_ref(),
                                            mention_roles.as_ref());
            let message = tryopt!(format_message(&channel,
                                                 id,
                                                 author.as_ref(),
                                                 content.as_ref().map(|x| &**x),
                                                 attachments.as_ref(),
                                                 "EDIT: ",
                                                 is_self));
            message.print(&channel.buffer);
        }
        Event::MessageDelete {
            message_id,
            channel_id,
        } => {
            let channel = tryopt!(ChannelData::from_discord_event(state, discord, channel_id));
            let message =
                tryopt!(format_message(&channel, message_id, None, None, None, "DELETE: ", false));
            message.print(&channel.buffer);
            on_delete(&channel, &message);
        }
        Event::ServerCreate(PossibleServer::Online(_)) |
        Event::ServerMemberUpdate { .. } |
        Event::ServerMemberAdd(_, _) |
        Event::ServerMemberRemove(_, _) |
        Event::ServerMembersChunk(_, _) |
        Event::ServerSync { .. } |
        Event::ChannelCreate(_) |
        Event::ChannelUpdate(_) |
        Event::ChannelDelete(_) |
        Event::PresenceUpdate { .. } => open_and_sync_buffers(state, discord),
        Event::Resumed { .. } |
        Event::UserUpdate(_) |
        Event::UserNoteUpdate(_, _) |
        Event::UserSettingsUpdate { .. } |
        Event::UserServerSettingsUpdate(_) |
        Event::VoiceStateUpdate(_, _) |
        Event::VoiceServerUpdate { .. } |
        Event::CallCreate(_) |
        Event::CallUpdate { .. } |
        Event::CallDelete(_) |
        Event::ChannelRecipientAdd(_, _) |
        Event::ChannelRecipientRemove(_, _) |
        Event::TypingStart { .. } |
        Event::PresencesReplace(_) |
        Event::RelationshipAdd(_) |
        Event::RelationshipRemove(_, _) |
        Event::MessageAck { .. } |
        Event::MessageDeleteBulk { .. } |
        Event::ServerCreate(PossibleServer::Offline(_)) |
        Event::ServerUpdate(_) |
        Event::ServerDelete(_) |
        Event::ServerRoleCreate(_, _) |
        Event::ServerRoleUpdate(_, _) |
        Event::ServerRoleDelete(_, _) |
        Event::ServerBanAdd(_, _) |
        Event::ServerBanRemove(_, _) |
        Event::ServerIntegrationsUpdate(_) |
        Event::ServerEmojisUpdate(_, _) |
        Event::ChannelPinsAck { .. } |
        Event::ChannelPinsUpdate { .. } |
        Event::Unknown(_, _) |
        _ => (),
    };
    Some(())
}

fn on_delete(channel: &ChannelData, message: &FormattedMessage) {
    if let ChannelRef::Public(server, _) = channel.channel {
        if let Some(dest_chan) = ffi::get_option(&format!("on_delete.{}", server.id.0))
               .and_then(|id| id.parse::<u64>().ok())
               .map(ChannelId) {
            if channel.state.find_channel(dest_chan).is_none() {
                return;
            }
            let message = format!("AUTO: Deleted message by {} in {}: {}",
                                  message.author,
                                  message.channel,
                                  message.content);
            let message = ffi::remove_color(&message);
            let result = channel.discord.send_message(dest_chan, &message, "", false);
            match result {
                Ok(_) => (),
                Err(err) => {
                    ffi::MAIN_BUFFER.print(&format!("Failed to send on_delete message: {}", err))
                }
            }
        }
    }
}
