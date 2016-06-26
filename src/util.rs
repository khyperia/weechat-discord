use discord::State;
use discord::model::{LiveServer, ServerId, Presence, UserId, User, Member};

pub trait StateExt {
    fn find_server(&self, server_id: &ServerId) -> Option<&LiveServer>;
}

impl StateExt for State {
    fn find_server(&self, server_id: &ServerId) -> Option<&LiveServer> {
        self.servers().iter().find(|s| s.id == *server_id)
    }
}

pub trait ServerExt {
    fn find_presence(&self, user_id: UserId) -> Option<&Presence>;
    fn find_member(&self, user_id: UserId) -> Option<&Member>;
    fn find_user(&self, user_id: UserId) -> Option<&User>;
}

impl ServerExt for LiveServer {
    fn find_presence(&self, user_id: UserId) -> Option<&Presence> {
        self.presences.iter().find(|p| p.user_id == user_id)
    }

    fn find_member(&self, user_id: UserId) -> Option<&Member> {
        self.members.iter().find(|p| p.user.id == user_id)
    }

    fn find_user(&self, user_id: UserId) -> Option<&User> {
        self.find_member(user_id).map(|m| &m.user)
    }
}
