use discord::*;
use discord::model::*;

use ffi;
use connection::*;
use message::*;
use types::*;

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
        Event::ServerCreate(PossibleServer::Online(ref server)) => {
            for channel in &server.channels {
                ChannelData::from_channel(state,
                                          discord,
                                          ChannelRef::Public(server, channel),
                                          true);
            }
        }
        Event::ServerMemberUpdate { .. } => {
            // ... this event is broken and I don't want to deal with it
        }
        Event::ServerMemberAdd(server_id, ref member) => {
            if let Some(server) = state.find_server(server_id) {
                for channel in &server.channels {
                    if let Some(chan) = ChannelData::from_channel(state,
                                                                  discord,
                                                                  ChannelRef::Public(server,
                                                                                     channel),
                                                                  false) {
                        chan.add_member(member)
                    }
                }
            }
        }
        Event::ServerMemberRemove(server_id, ref user) => {
            if let Some(server) = state.find_server(server_id) {
                // why the HECK is this a user and not a member!?!?!
                let mut member = None;
                for mem in &server.members {
                    if mem.id() == user.id() {
                        member = Some(mem);
                        break;
                    }
                }
                if let Some(member) = member {
                    for channel in &server.channels {
                        if let Some(chan) = ChannelData::from_channel(state,
                                                                      discord,
                                                                      ChannelRef::Public(server,
                                                                                         channel),
                                                                      false) {
                            chan.remove_member(member)
                        }
                    }
                }
            }
        }
        Event::ServerMembersChunk(server_id, ref members) |
        Event::ServerSync {
            server_id,
            ref members,
            ..
        } => {
            if let Some(server) = state.find_server(server_id) {
                for channel in &server.channels {
                    if let Some(chan) = ChannelData::from_channel(state,
                                                                  discord,
                                                                  ChannelRef::Public(server,
                                                                                     channel),
                                                                  false) {
                        for member in members {
                            chan.add_member(member)
                        }
                    }
                }
            }
        }
        Event::ChannelCreate(ref channel) => {
            let channel_ref = match *channel {
                Channel::Public(ref public) => {
                    if let Some(server) = state.find_server(public.server_id) {
                        ChannelRef::Public(server, public)
                    } else {
                        return Some(());
                    }
                }
                Channel::Group(ref group) => ChannelRef::Group(group),
                Channel::Private(ref private) => ChannelRef::Private(private),
            };
            ChannelData::from_channel(state, discord, channel_ref, true);
        }
        Event::ChannelUpdate(ref channel) => {
            let channel_ref = match *channel {
                Channel::Public(ref public) => {
                    if let Some(server) = state.find_server(public.server_id) {
                        ChannelRef::Public(server, public)
                    } else {
                        return Some(());
                    }
                }
                Channel::Group(ref group) => ChannelRef::Group(group),
                Channel::Private(ref private) => ChannelRef::Private(private),
            };
            ChannelData::from_channel(state, discord, channel_ref, false);
        }
        Event::UserServerSettingsUpdate(ref settings) => ChannelData::mute_channels(settings),
        Event::CallCreate(_) |
        Event::CallDelete(_) |
        Event::CallUpdate { .. } |
        Event::ChannelDelete(_) |
        Event::ChannelPinsAck { .. } |
        Event::ChannelPinsUpdate { .. } |
        Event::ChannelRecipientAdd(_, _) |
        Event::ChannelRecipientRemove(_, _) |
        Event::MessageAck { .. } |
        Event::MessageDeleteBulk { .. } |
        Event::PresenceUpdate { .. } |
        Event::PresencesReplace(_) |
        Event::ReactionAdd(_) |
        Event::ReactionRemove(_) |
        Event::Ready(_) |
        Event::RelationshipAdd(_) |
        Event::RelationshipRemove(_, _) |
        Event::Resumed { .. } |
        Event::ServerBanAdd(_, _) |
        Event::ServerBanRemove(_, _) |
        Event::ServerCreate(PossibleServer::Offline(_)) |
        Event::ServerDelete(_) |
        Event::ServerEmojisUpdate(_, _) |
        Event::ServerIntegrationsUpdate(_) |
        Event::ServerRoleCreate(_, _) |
        Event::ServerRoleDelete(_, _) |
        Event::ServerRoleUpdate(_, _) |
        Event::ServerUpdate(_) |
        Event::TypingStart { .. } |
        Event::UserNoteUpdate(_, _) |
        Event::UserSettingsUpdate { .. } |
        Event::UserUpdate(_) |
        Event::VoiceServerUpdate { .. } |
        Event::VoiceStateUpdate(_, _) |
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
