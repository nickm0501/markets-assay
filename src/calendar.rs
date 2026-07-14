use chrono::Datelike;
use chrono::{DateTime, Duration, NaiveDate, NaiveTime, TimeZone, Utc, Weekday};
use chrono_tz::America::New_York;
use std::collections::BTreeMap;

const REGULAR_OPEN_HOUR: u32 = 9;
const REGULAR_OPEN_MINUTE: u32 = 30;
const REGULAR_CLOSE_HOUR: u32 = 16;

pub fn is_trading_day(date: NaiveDate, holidays: &[NaiveDate]) -> bool {
    !matches!(date.weekday(), Weekday::Sat | Weekday::Sun) && !holidays.contains(&date)
}

/// NYSE hours are defined in America/New_York local time and converted to
/// UTC per date, so the UTC offset automatically follows DST transitions
/// instead of assuming a fixed offset (spec: "respect ... daylight saving
/// transitions").
fn local_time_to_utc(date: NaiveDate, time: NaiveTime) -> DateTime<Utc> {
    New_York
        .from_local_datetime(&date.and_time(time))
        .single()
        .expect("NYSE open/close/early-close times never fall in a DST fold or gap")
        .with_timezone(&Utc)
}

pub fn regular_open(date: NaiveDate) -> DateTime<Utc> {
    local_time_to_utc(
        date,
        NaiveTime::from_hms_opt(REGULAR_OPEN_HOUR, REGULAR_OPEN_MINUTE, 0).unwrap(),
    )
}

/// `early_closes` maps a date to its local "HH:MM" NYSE closing time (e.g.
/// "13:00" for a 1:00pm ET half day). Dates absent from the map close at the
/// regular 4:00pm ET time.
pub fn regular_close(date: NaiveDate, early_closes: &BTreeMap<NaiveDate, String>) -> DateTime<Utc> {
    if let Some(local_close) = early_closes.get(&date) {
        let time = NaiveTime::parse_from_str(local_close, "%H:%M").unwrap_or_else(|_| {
            panic!("early_closes value for {date} must be HH:MM local time, got {local_close}")
        });
        return local_time_to_utc(date, time);
    }
    local_time_to_utc(
        date,
        NaiveTime::from_hms_opt(REGULAR_CLOSE_HOUR, 0, 0).unwrap(),
    )
}

pub fn is_regular_session(
    time: DateTime<Utc>,
    holidays: &[NaiveDate],
    early_closes: &BTreeMap<NaiveDate, String>,
) -> bool {
    let date = time.date_naive();
    is_trading_day(date, holidays)
        && time >= regular_open(date)
        && time < regular_close(date, early_closes)
}

pub fn next_regular_signal_time(
    time: DateTime<Utc>,
    interval_minutes: i64,
    holidays: &[NaiveDate],
    early_closes: &BTreeMap<NaiveDate, String>,
) -> DateTime<Utc> {
    let mut date = time.date_naive();
    loop {
        if is_trading_day(date, holidays) {
            let open = regular_open(date);
            let close = regular_close(date, early_closes);
            if time <= open {
                return open;
            }
            if time < close {
                let elapsed = time - open;
                let intervals = (elapsed.num_minutes() + interval_minutes - 1) / interval_minutes;
                return open + Duration::minutes(intervals * interval_minutes);
            }
        }
        date = date.succ_opt().unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_regular_signal_skips_weekend_and_fixture_holiday() {
        use chrono::{TimeZone, Utc};
        use std::collections::BTreeMap;

        let holidays = vec!["2026-07-03".parse().unwrap()];
        let early_closes = BTreeMap::new();
        let after_close = Utc.with_ymd_and_hms(2026, 7, 2, 21, 15, 0).unwrap();
        let next = next_regular_signal_time(after_close, 60, &holidays, &early_closes);

        assert_eq!(next.to_rfc3339(), "2026-07-06T13:30:00+00:00");
    }

    #[test]
    fn regular_open_uses_est_offset_outside_daylight_saving() {
        // Required because calendar.rs must respect DST transitions, not just a
        // fixed UTC offset (spec Data Quality And Error Handling, Required Tests).
        let date = "2026-01-05".parse().unwrap();
        assert_eq!(regular_open(date).to_rfc3339(), "2026-01-05T14:30:00+00:00");
    }

    #[test]
    fn regular_open_uses_edt_offset_during_daylight_saving() {
        let date = "2026-07-06".parse().unwrap();
        assert_eq!(regular_open(date).to_rfc3339(), "2026-07-06T13:30:00+00:00");
    }

    #[test]
    fn regular_close_honors_configured_early_close() {
        use std::collections::BTreeMap;

        let date: chrono::NaiveDate = "2026-07-02".parse().unwrap();
        let mut early_closes = BTreeMap::new();
        early_closes.insert(date, "13:00".to_string());

        assert_eq!(
            regular_close(date, &early_closes).to_rfc3339(),
            "2026-07-02T17:00:00+00:00"
        );
    }
}
