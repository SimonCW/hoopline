use chrono::{DateTime, Datelike, Timelike, Utc};

pub const MAX_PLAYERS: usize = 15;
pub const MAX_WAITLIST: usize = 5;

#[derive(Clone, Debug)]
pub struct Slot {
    pub id: i64,
    pub datetime: String,
    pub venue: String,
    pub players: Vec<String>,
    pub player_user_ids: Vec<i64>,
    pub waitlist: Vec<String>,
    pub waitlist_user_ids: Vec<i64>,
}

impl Slot {
    pub fn datetime_label_de(&self) -> String {
        let Some(parsed) = self.parsed_datetime() else {
            return self.datetime.clone();
        };
        format!(
            "{}, {:02}.{:02}.{:02} · {:02}:{:02}",
            weekday_short_de(parsed.weekday()),
            parsed.day(),
            parsed.month(),
            parsed.year_ce().1 % 100,
            parsed.hour(),
            parsed.minute()
        )
    }

    pub fn user_is_signed_up(&self, current_user_id: &Option<i64>) -> bool {
        current_user_id.is_some_and(|user_id| {
            self.player_user_ids.contains(&user_id) || self.waitlist_user_ids.contains(&user_id)
        })
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

    pub fn player_is_current_user(&self, index: &usize, current_user_id: &Option<i64>) -> bool {
        current_user_id.is_some_and(|user_id| self.player_user_id(index) == user_id)
    }

    pub fn waitlist_is_current_user(&self, index: &usize, current_user_id: &Option<i64>) -> bool {
        current_user_id.is_some_and(|user_id| self.waitlist_user_id(index) == user_id)
    }

    pub fn player_has_booking(&self, index: &usize) -> bool {
        self.players.get(*index).is_some()
    }

    pub fn waitlist_has_booking(&self, index: &usize) -> bool {
        self.waitlist.get(*index).is_some()
    }

    pub fn can_promote_from_waitlist(&self) -> bool {
        self.players.len() < MAX_PLAYERS
    }

    pub fn player_name(&self, index: &usize) -> &str {
        self.players.get(*index).map_or("-", String::as_str)
    }

    pub fn waitlist_name(&self, index: &usize) -> &str {
        self.waitlist.get(*index).map_or("-", String::as_str)
    }

    pub fn player_user_id(&self, index: &usize) -> i64 {
        self.player_user_ids
            .get(*index)
            .copied()
            .unwrap_or_default()
    }

    pub fn waitlist_user_id(&self, index: &usize) -> i64 {
        self.waitlist_user_ids
            .get(*index)
            .copied()
            .unwrap_or_default()
    }

    fn parsed_datetime(&self) -> Option<DateTime<Utc>> {
        DateTime::parse_from_rfc3339(&self.datetime)
            .ok()
            .map(|value| value.with_timezone(&Utc))
    }
}

fn weekday_short_de(day: chrono::Weekday) -> &'static str {
    match day {
        chrono::Weekday::Mon => "Mo",
        chrono::Weekday::Tue => "Di",
        chrono::Weekday::Wed => "Mi",
        chrono::Weekday::Thu => "Do",
        chrono::Weekday::Fri => "Fr",
        chrono::Weekday::Sat => "Sa",
        chrono::Weekday::Sun => "So",
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserIdentity {
    pub id: i64,
    pub name: String,
    pub is_admin: bool,
}
