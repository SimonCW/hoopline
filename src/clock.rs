//! Application wall time, with a fixed clock for local scenario builds.

use chrono::{DateTime, Utc};

pub(crate) fn now() -> DateTime<Utc> {
    #[cfg(feature = "scenario-tests")]
    if let Ok(value) = std::env::var("HOOPLINE_TEST_NOW") {
        return parse(&value);
    }
    Utc::now()
}

#[cfg(feature = "scenario-tests")]
fn parse(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .expect("HOOPLINE_TEST_NOW must be an RFC3339 timestamp")
        .with_timezone(&Utc)
}

#[cfg(test)]
mod tests {
    #[test]
    fn unset_clock_tracks_wall_time() {
        // No test changes the process environment.
        if std::env::var_os("HOOPLINE_TEST_NOW").is_none() {
            let before = chrono::Utc::now();
            let actual = super::now();
            let after = chrono::Utc::now();
            assert!((before..=after).contains(&actual));
        }
    }

    #[cfg(feature = "scenario-tests")]
    #[test]
    fn offsets_represent_the_same_instant() {
        assert_eq!(
            super::parse("2026-11-08T19:00:00+01:00"),
            super::parse("2026-11-08T18:00:00Z")
        );
    }

    #[cfg(feature = "scenario-tests")]
    #[test]
    #[should_panic(expected = "HOOPLINE_TEST_NOW must be an RFC3339 timestamp")]
    fn invalid_fixed_time_fails_closed() {
        super::parse("not a timestamp");
    }
}
