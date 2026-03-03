#[derive(Clone, Debug)]
pub struct Slot {
    pub datetime: String,
    pub venue: String,
    pub players: Vec<String>,
    pub waitlist: Vec<String>,
}

impl Slot {
    pub fn sample_slots() -> Vec<Self> {
        vec![
            Self {
                datetime: "Mon 2026-03-09 20:00-22:00".to_string(),
                venue: "Court A".to_string(),
                players: vec![
                    "Alex".to_string(),
                    "Ben".to_string(),
                    "Chris".to_string(),
                    "Dani".to_string(),
                    "Eli".to_string(),
                    "Farid".to_string(),
                ],
                waitlist: vec!["Gio".to_string(), "Hana".to_string()],
            },
            Self {
                datetime: "Tue 2026-03-10 20:00-21:45".to_string(),
                venue: "Court B".to_string(),
                players: vec![
                    "Ira".to_string(),
                    "Jamal".to_string(),
                    "Kai".to_string(),
                    "Luca".to_string(),
                    "Maya".to_string(),
                    "Nico".to_string(),
                    "Omar".to_string(),
                    "Pia".to_string(),
                ],
                waitlist: vec!["Quinn".to_string()],
            },
            Self {
                datetime: "Thu 2026-03-12 20:00-22:00".to_string(),
                venue: "Court C".to_string(),
                players: vec![
                    "Rae".to_string(),
                    "Sam".to_string(),
                    "Toni".to_string(),
                    "Uma".to_string(),
                    "Vik".to_string(),
                ],
                waitlist: vec!["Will".to_string(), "Xena".to_string(), "Yara".to_string()],
            },
        ]
    }

    pub fn player_name(&self, index: &usize) -> &str {
        self.players
            .get(*index)
            .map(|name| name.as_str())
            .unwrap_or("-")
    }

    pub fn waitlist_name(&self, index: &usize) -> &str {
        self.waitlist
            .get(*index)
            .map(|name| name.as_str())
            .unwrap_or("-")
    }
}
