pub const MAX_PLAYERS: usize = 15;
pub const MAX_WAITLIST: usize = 5;

#[derive(Clone, Debug)]
pub struct Slot {
    pub id: i64,
    pub datetime: String,
    pub venue: String,
    pub players: Vec<String>,
    pub waitlist: Vec<String>,
}

impl Slot {
    pub fn user_is_signed_up(&self, current_user_name: &str) -> bool {
        !current_user_name.is_empty()
            && (self.players.iter().any(|name| name == current_user_name)
                || self.waitlist.iter().any(|name| name == current_user_name))
    }

    pub fn can_accept_signup(&self) -> bool {
        self.players.len() < MAX_PLAYERS || self.waitlist.len() < MAX_WAITLIST
    }

    pub fn signup_action_label(&self) -> &'static str {
        if self.players.len() < MAX_PLAYERS {
            "Sign Up"
        } else {
            "Join Waitlist"
        }
    }

    pub fn player_is_current_user(&self, index: &usize, current_user_name: &str) -> bool {
        !current_user_name.is_empty() && self.player_name(index) == current_user_name
    }

    pub fn waitlist_is_current_user(&self, index: &usize, current_user_name: &str) -> bool {
        !current_user_name.is_empty() && self.waitlist_name(index) == current_user_name
    }

    pub fn player_name(&self, index: &usize) -> &str {
        self.players.get(*index).map_or("-", String::as_str)
    }

    pub fn waitlist_name(&self, index: &usize) -> &str {
        self.waitlist.get(*index).map_or("-", String::as_str)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserIdentity {
    pub id: i64,
    pub name: String,
}
