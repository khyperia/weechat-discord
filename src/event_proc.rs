use discord::*;
use discord::model::*;

use ffi;
use types::*;
use connection::*;
use message::*;

pub fn open_and_sync_buffers(state: &RcState, sender: &OutgoingPipe) {
    for server in state.read().unwrap().servers() {
        for channel in &server.channels {
            if channel.kind == ChannelType::Voice {
                continue;
            }
            if let Some(buffer) = ChannelData::create(state, &sender, channel.id) {
                for member in &server.members {
                    let name = member.user.name(&NameFormat::none());
                    if !buffer.nick_exists(&name) {
                        buffer.add_nick(&name);
                    }
                }
            };
        }
    }
}

pub fn on_event(state: &RcState, sender: &OutgoingPipe, event: Event) {
    match event {
        Event::MessageCreate(ref message) => {
            let is_self = is_self_mentioned(&state.read().unwrap(),
                                            message.channel_id,
                                            message.mention_everyone,
                                            Some(&message.mentions),
                                            Some(&message.mention_roles));
            let message = format_message(&state.read().unwrap(),
                                         message.channel_id,
                                         message.id,
                                         Some(&message.author),
                                         Some(&message.content),
                                         Some(&message.attachments),
                                         "",
                                         is_self,
                                         false);
            message.map(|m| m.print());
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
            let is_self = is_self_mentioned(&state.read().unwrap(),
                                            channel_id,
                                            mention_everyone.unwrap_or(false),
                                            mentions.as_ref(),
                                            mention_roles.as_ref());
            let message = format_message(&state.read().unwrap(),
                                         channel_id,
                                         id,
                                         author.as_ref(),
                                         content.as_ref().map(|x| &**x),
                                         attachments.as_ref(),
                                         "EDIT: ",
                                         is_self,
                                         false);
            message.map(|m| m.print());
        }
        Event::MessageDelete {
            message_id,
            channel_id,
        } => {
            let message = format_message(&state.read().unwrap(),
                                         channel_id,
                                         message_id,
                                         None,
                                         None,
                                         None,
                                         "DELETE: ",
                                         false,
                                         false);
            message.map(|m| {
                            m.print();
                            on_delete(state, sender, channel_id, m);
                        });
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
        Event::PresenceUpdate { .. } => open_and_sync_buffers(state, sender),
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
    }
}

fn on_delete(state: &RcState,
             outgoing: &OutgoingPipe,
             source_chan: ChannelId,
             message: FormattedMessage) {
    if let Some(ChannelRef::Public(server, _)) = state.read().unwrap().find_channel(source_chan) {
        if let Some(dest_chan) = ffi::get_option(&format!("on_delete.{}", server.id.0))
               .and_then(|id| id.parse::<u64>().ok())
               .map(|id| ChannelId(id)) {
            if state.read().unwrap().find_channel(dest_chan).is_none() {
                return;
            }
            let message = format!("AUTO: Deleted message by {} in {}: {}",
                                  message.author,
                                  message.channel,
                                  message.content);
            let message = ffi::remove_color(&message);
            let result = outgoing
                .discord
                .send_message(dest_chan, &message, "", false);
            match result {
                Ok(_) => (),
                Err(err) => {
                    ffi::MAIN_BUFFER.print(&format!("Failed to send on_delete message: {}", err))
                }
            }
        }
    }
}
