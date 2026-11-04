//! Calendar rules for opening recurring games.

use chrono::{DateTime, Datelike, Days, NaiveDate, NaiveTime, TimeZone, Utc, Weekday};
use chrono_tz::Europe::Berlin;

/// Returns the last game date open under the Sunday release rule.
///
/// The Sunday release opens the week beginning eight days later.
///
/// # Errors
///
/// Returns an error if the release time is ambiguous or outside the date range.
pub fn latest_open_date(
    now: DateTime<Utc>,
    release_time: NaiveTime,
) -> Result<NaiveDate, &'static str> {
    let local = now.with_timezone(&Berlin);
    let today = local.date_naive();
    let days_since_sunday = u64::from(today.weekday().num_days_from_sunday());
    let sunday = today
        .checked_sub_days(Days::new(days_since_sunday))
        .ok_or("release date is outside the supported range")?;
    let release = Berlin
        .from_local_datetime(&sunday.and_time(release_time))
        .single()
        .ok_or("release time is ambiguous or does not exist")?;
    let last_release = if local < release {
        sunday.checked_sub_days(Days::new(7))
    } else {
        Some(sunday)
    }
    .ok_or("release date is outside the supported range")?;
    last_release
        .checked_add_days(Days::new(14))
        .ok_or("game date is outside the supported range")
}

/// Returns the next release and the Monday of its target game week.
///
/// # Errors
///
/// Returns an error if the release time is ambiguous or outside the date range.
pub fn next_release(
    now: DateTime<Utc>,
    release_time: NaiveTime,
) -> Result<(DateTime<Utc>, NaiveDate), &'static str> {
    let local = now.with_timezone(&Berlin);
    let today = local.date_naive();
    let days_until_sunday = u64::from((7 - today.weekday().num_days_from_sunday()) % 7);
    let mut sunday = today
        .checked_add_days(Days::new(days_until_sunday))
        .ok_or("release date is outside the supported range")?;
    let mut release = Berlin
        .from_local_datetime(&sunday.and_time(release_time))
        .single()
        .ok_or("release time is ambiguous or does not exist")?;
    if release <= local {
        sunday = sunday
            .checked_add_days(Days::new(7))
            .ok_or("release date is outside the supported range")?;
        release = Berlin
            .from_local_datetime(&sunday.and_time(release_time))
            .single()
            .ok_or("release time is ambiguous or does not exist")?;
    }
    debug_assert_eq!(sunday.weekday(), Weekday::Sun);
    let monday = sunday
        .checked_add_days(Days::new(8))
        .ok_or("game date is outside the supported range")?;
    Ok((release.with_timezone(&Utc), monday))
}

#[cfg(test)]
mod tests {
    use super::{latest_open_date, next_release};
    use chrono::{Days, NaiveDate, NaiveTime, TimeZone, Utc};

    #[test]
    fn sunday_release_opens_the_week_starting_eight_days_later() {
        let time = NaiveTime::from_hms_opt(19, 0, 0).unwrap();
        let before = Utc.with_ymd_and_hms(2026, 11, 8, 17, 59, 59).unwrap();
        let at = Utc.with_ymd_and_hms(2026, 11, 8, 18, 0, 0).unwrap();
        assert_eq!(
            latest_open_date(before, time).unwrap(),
            NaiveDate::from_ymd_opt(2026, 11, 15).unwrap()
        );
        assert_eq!(
            latest_open_date(at, time).unwrap(),
            NaiveDate::from_ymd_opt(2026, 11, 22).unwrap()
        );
        assert_eq!(
            next_release(before, time).unwrap(),
            (at, NaiveDate::from_ymd_opt(2026, 11, 16).unwrap())
        );
        assert_eq!(
            next_release(at, time).unwrap().1,
            NaiveDate::from_ymd_opt(2026, 11, 23).unwrap()
        );
    }

    #[test]
    fn release_uses_berlin_time_across_daylight_saving_and_year_end() {
        let time = NaiveTime::from_hms_opt(19, 0, 0).unwrap();
        let summer = Utc.with_ymd_and_hms(2026, 3, 29, 17, 0, 0).unwrap();
        assert_eq!(
            latest_open_date(summer, time).unwrap(),
            NaiveDate::from_ymd_opt(2026, 4, 12).unwrap()
        );
        let year_end = Utc.with_ymd_and_hms(2026, 12, 27, 18, 0, 0).unwrap();
        assert_eq!(
            latest_open_date(year_end, time).unwrap(),
            NaiveDate::from_ymd_opt(2027, 1, 10).unwrap()
        );
    }

    #[test]
    fn rejects_missing_or_ambiguous_release_time() {
        let time = NaiveTime::from_hms_opt(2, 30, 0).unwrap();
        for now in [
            Utc.with_ymd_and_hms(2026, 3, 29, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2026, 10, 25, 0, 0, 0).unwrap(),
        ] {
            assert!(latest_open_date(now, time).is_err());
            assert!(next_release(now, time).is_err());
        }
    }

    #[test]
    fn releases_use_the_new_offset_on_both_dst_change_days() {
        let time = NaiveTime::from_hms_opt(19, 0, 0).unwrap();
        for (release, target) in [
            (
                Utc.with_ymd_and_hms(2026, 3, 29, 17, 0, 0).unwrap(),
                NaiveDate::from_ymd_opt(2026, 4, 6).unwrap(),
            ),
            (
                Utc.with_ymd_and_hms(2026, 10, 25, 18, 0, 0).unwrap(),
                NaiveDate::from_ymd_opt(2026, 11, 2).unwrap(),
            ),
        ] {
            let before = release - chrono::Duration::seconds(1);
            assert_eq!(next_release(before, time).unwrap(), (release, target));
            assert_eq!(
                latest_open_date(release, time).unwrap(),
                target + chrono::Days::new(6)
            );
            assert_eq!(
                latest_open_date(before, time).unwrap(),
                target - chrono::Days::new(1)
            );
        }
    }

    #[test]
    fn every_sunday_releases_the_following_week() {
        // Use 2027 dates so the fixtures do not match the shipped example week.
        let time = NaiveTime::from_hms_opt(19, 0, 0).unwrap();
        for (sunday, first, last) in [
            (
                Utc.with_ymd_and_hms(2027, 8, 1, 17, 0, 0).unwrap(),
                NaiveDate::from_ymd_opt(2027, 8, 9).unwrap(),
                NaiveDate::from_ymd_opt(2027, 8, 15).unwrap(),
            ),
            (
                Utc.with_ymd_and_hms(2027, 8, 8, 17, 0, 0).unwrap(),
                NaiveDate::from_ymd_opt(2027, 8, 16).unwrap(),
                NaiveDate::from_ymd_opt(2027, 8, 22).unwrap(),
            ),
        ] {
            let before = sunday - chrono::Duration::seconds(1);
            assert_eq!(
                latest_open_date(before, time).unwrap(),
                first - Days::new(1)
            );
            assert_eq!(latest_open_date(sunday, time).unwrap(), last);
            assert_eq!(next_release(before, time).unwrap(), (sunday, first));
            assert_eq!(next_release(sunday, time).unwrap().1, first + Days::new(7));
        }
    }

    #[test]
    fn cutoff_stays_fixed_after_release_until_the_next_sunday() {
        let time = NaiveTime::from_hms_opt(19, 0, 0).unwrap();
        let expected = NaiveDate::from_ymd_opt(2026, 11, 22).unwrap();
        for now in [
            Utc.with_ymd_and_hms(2026, 11, 8, 18, 0, 1).unwrap(),
            Utc.with_ymd_and_hms(2026, 11, 9, 12, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2026, 11, 14, 12, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2026, 11, 15, 17, 59, 59).unwrap(),
        ] {
            assert_eq!(latest_open_date(now, time).unwrap(), expected);
        }
    }
}
