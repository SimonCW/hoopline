#[derive(Clone, Debug)]
pub struct Slot {
    pub id: i64,
    pub datetime: String,
    pub venue: String,
    pub players: Vec<String>,
    pub waitlist: Vec<String>,
}

impl Slot {
    pub fn player_name(&self, index: &usize) -> &str {
        self.players.get(*index).map_or("-", String::as_str)
    }

    pub fn waitlist_name(&self, index: &usize) -> &str {
        self.waitlist.get(*index).map_or("-", String::as_str)
    }
}
