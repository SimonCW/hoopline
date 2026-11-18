use chrono::{DateTime, Datelike, NaiveDate};
use chrono_tz::{Europe::Berlin, Tz};

#[derive(Clone, Debug)]
pub struct Slot {
    pub id: i64,
    pub revision: i64,
    pub is_override: bool,
    pub datetime: String,
    pub venue: String,
    pub map_url: String,
    pub max_players: usize,
    pub max_waitlist: usize,
    pub players: Vec<String>,
    pub player_user_ids: Vec<i64>,
    pub waitlist: Vec<String>,
    pub waitlist_user_ids: Vec<i64>,
}

#[derive(Clone, Debug)]
pub struct SlotWeek {
    pub monday: Option<NaiveDate>,
    pub key: String,
    pub slots: Vec<Slot>,
}

pub fn group_slots(slots: Vec<Slot>) -> Vec<SlotWeek> {
    let mut weeks: Vec<SlotWeek> = Vec::new();
    for slot in slots {
        let key = slot.week_key();
        if weeks.last().is_none_or(|week| week.key != key) {
            weeks.push(SlotWeek {
                monday: slot.week_start(),
                key,
                slots: Vec::new(),
            });
        }
        if let Some(week) = weeks.last_mut() {
            week.slots.push(slot);
        }
    }
    weeks
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BookingStatus {
    Waiting(usize),
    Confirmed,
    Started,
    Open,
    Full,
}

impl Slot {
    pub fn edit_date(&self) -> String {
        self.parsed_datetime()
            .map_or_else(String::new, |value| value.format("%Y-%m-%d").to_string())
    }

    pub fn edit_time(&self) -> String {
        self.parsed_datetime()
            .map_or_else(String::new, |value| value.format("%H:%M").to_string())
    }
    pub fn has_user(&self, user_id: &i64) -> bool {
        self.player_user_ids.contains(user_id) || self.waitlist_user_ids.contains(user_id)
    }

    pub fn user_is_signed_up(&self, current_user_id: &Option<i64>) -> bool {
        current_user_id.is_some_and(|user_id| {
            self.player_user_ids.contains(&user_id) || self.waitlist_user_ids.contains(&user_id)
        })
    }

    pub fn user_is_waitlisted(&self, current_user_id: &Option<i64>) -> bool {
        current_user_id.is_some_and(|user_id| self.waitlist_user_ids.contains(&user_id))
    }

    pub fn can_accept_signup(&self) -> bool {
        !self.has_started()
            && (self.players.len() < self.max_players || self.waitlist.len() < self.max_waitlist)
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
        self.players.len() < self.max_players
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

    pub fn has_started(&self) -> bool {
        self.parsed_datetime()
            .is_none_or(|time| time <= crate::clock::now())
    }

    pub(crate) fn booking_status(&self, user_id: Option<i64>) -> BookingStatus {
        if let Some(position) =
            user_id.and_then(|id| self.waitlist_user_ids.iter().position(|user| *user == id))
        {
            return BookingStatus::Waiting(position + 1);
        }
        if user_id.is_some_and(|id| self.player_user_ids.contains(&id)) {
            return BookingStatus::Confirmed;
        }
        if self.has_started() {
            BookingStatus::Started
        } else if self.can_accept_signup() {
            BookingStatus::Open
        } else {
            BookingStatus::Full
        }
    }

    pub fn week_key(&self) -> String {
        self.parsed_datetime()
            .map_or_else(String::new, |time| time.format("%G-W%V").to_string())
    }

    fn week_start(&self) -> Option<NaiveDate> {
        let time = self.parsed_datetime()?;
        time.date_naive()
            .checked_sub_days(chrono::Days::new(u64::from(
                time.weekday().num_days_from_monday(),
            )))
    }

    pub(crate) fn parsed_datetime(&self) -> Option<DateTime<Tz>> {
        DateTime::parse_from_rfc3339(&self.datetime)
            .ok()
            .map(|value| value.with_timezone(&Berlin))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserIdentity {
    pub id: i64,
    pub name: String,
    pub is_admin: bool,
}

#[cfg(test)]
mod tests {
    use super::{BookingStatus, Slot, group_slots};
    use crate::i18n::Locale;

    fn slot(datetime: &str) -> Slot {
        Slot {
            id: 1,
            revision: 0,
            is_override: false,
            datetime: datetime.into(),
            venue: "Court".into(),
            map_url: String::new(),
            max_players: 1,
            max_waitlist: 1,
            players: vec!["Alex".into()],
            player_user_ids: vec![1],
            waitlist: vec!["Ben".into()],
            waitlist_user_ids: vec![2],
        }
    }

    #[test]
    fn status_and_capacity_use_the_slot_limits() {
        let mut slot = slot("2099-01-05T19:00:00Z");
        assert_eq!(slot.booking_status(Some(1)), BookingStatus::Confirmed);
        assert_eq!(slot.booking_status(Some(2)), BookingStatus::Waiting(1));
        assert_eq!(slot.booking_status(Some(3)), BookingStatus::Full);
        assert!(!slot.can_accept_signup());
        assert!(!slot.can_promote_from_waitlist());
        slot.max_players = 2;
        assert!(slot.can_accept_signup());
        assert!(slot.can_promote_from_waitlist());
        assert_eq!(slot.booking_status(Some(3)), BookingStatus::Open);
    }

    #[test]
    fn dates_use_local_time_and_iso_week_year() {
        assert_eq!(
            Locale::De.game_time(&slot("2030-03-25T19:00:00Z")),
            "Mo., 25.03.2030 · 20:00 Uhr"
        );
        assert_eq!(
            Locale::De.game_time(&slot("2030-04-01T18:00:00Z")),
            "Mo., 01.04.2030 · 20:00 Uhr"
        );
        let weeks = group_slots(vec![
            slot("2029-12-30T19:00:00Z"),
            slot("2029-12-31T19:00:00Z"),
            slot("2030-01-01T19:00:00Z"),
        ]);
        assert_eq!(weeks.len(), 2);
        assert_eq!(weeks[1].key, "2030-W01");
        assert_eq!(weeks[1].slots.len(), 2);
    }

    #[test]
    fn past_or_invalid_times_cannot_accept_bookings() {
        assert!(!slot("2000-01-01T20:00:00Z").can_accept_signup());
        assert!(slot("not a date").has_started());
    }
}
