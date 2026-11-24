use chrono::{DateTime, NaiveDate, TimeZone};
use ratatui::style::Style;

use super::theme::Theme;
use crate::todoist::model::Due;

pub struct FormattedDue {
    pub text: String,
    pub style: Style,
}

/// Where a due date falls relative to today. Timed dues count by their
/// calendar day, so a task due at 09:00 today is due today all day; a due
/// with a fixed timezone counts by its day in the user's zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DueBucket {
    Overdue,
    Today,
    /// The six days after today.
    ThisWeek,
    Later,
    /// No due date, or one that does not parse.
    NoDate,
}

/// The bucket `due` falls in on `today`, in `zone`.
pub fn due_bucket<Tz: TimeZone>(due: Option<&Due>, today: NaiveDate, zone: &Tz) -> DueBucket {
    match due.and_then(|due| due_day(due, zone)) {
        None => DueBucket::NoDate,
        Some(day) => match (day - today).num_days() {
            days if days < 0 => DueBucket::Overdue,
            0 => DueBucket::Today,
            1..=6 => DueBucket::ThisWeek,
            _ => DueBucket::Later,
        },
    }
}

/// The calendar day `due` falls on in `zone`. A floating or all-day due
/// names its day as written; a due with a fixed timezone is an instant
/// (`2026-11-23T05:30:00Z`), which falls on its day in `zone`.
pub fn due_day<Tz: TimeZone>(due: &Due, zone: &Tz) -> Option<NaiveDate> {
    if let Ok(instant) = DateTime::parse_from_rfc3339(&due.date) {
        return Some(instant.with_timezone(zone).date_naive());
    }
    NaiveDate::parse_from_str(date_part(&due.date), "%Y-%m-%d").ok()
}

/// How `due` reads and looks on `today`, in `zone`.
pub fn format_due<Tz: TimeZone>(
    due: &Due,
    today: NaiveDate,
    zone: &Tz,
    theme: &Theme,
) -> FormattedDue {
    let style = match due_bucket(Some(due), today, zone) {
        DueBucket::Overdue => theme.due_overdue(),
        DueBucket::Today => theme.due_today(),
        DueBucket::ThisWeek => theme.due_upcoming(),
        DueBucket::Later | DueBucket::NoDate => theme.due_future(),
    };
    FormattedDue {
        text: display_label(due, due_day(due, zone), today),
        style,
    }
}

/// The day `due` falls on, relative to today where that is near, and its
/// time when it has one: `today`, `tomorrow 9:30`, `Nov 28`. Never the due
/// string the user typed: that names the day relative to when it was typed
/// ("tomorrow" goes stale the next day) and, for a recurring task, the rule
/// rather than the next date. The detail shows the rule beside the date.
fn display_label(due: &Due, day: Option<NaiveDate>, today: NaiveDate) -> String {
    let day = match day {
        Some(day) => match (day - today).num_days() {
            0 => "today".to_string(),
            1 => "tomorrow".to_string(),
            -1 => "yesterday".to_string(),
            _ => day.format("%b %-d").to_string(),
        },
        None => date_part(&due.date).to_string(),
    };
    match time_part(due) {
        Some(time) => format!("{day} {time}"),
        None => day,
    }
}

/// The time of a timed due, as `9:30`, or `None` for an all-day due. A
/// time in UTC (ending in `Z`, a due with a fixed timezone) is left out
/// rather than shown as if it were local.
fn time_part(due: &Due) -> Option<String> {
    let stamp = due.datetime.as_deref().unwrap_or(&due.date);
    let (_, time) = stamp.split_once('T')?;
    if time.ends_with('Z') {
        return None;
    }
    let parsed = chrono::NaiveTime::parse_from_str(time, "%H:%M:%S")
        .or_else(|_| chrono::NaiveTime::parse_from_str(time, "%H:%M"))
        .ok()?;
    Some(parsed.format("%-H:%M").to_string())
}

/// The calendar-date portion of a Todoist due date, which may be a bare
/// `YYYY-MM-DD` or a full `YYYY-MM-DDTHH:MM:SS` timestamp.
pub fn date_part(due_date: &str) -> &str {
    due_date.split('T').next().unwrap_or(due_date)
}

#[cfg(test)]
mod tests {
    use chrono::{FixedOffset, NaiveDate, Utc};

    use super::{DueBucket, date_part, due_bucket, format_due};
    use crate::todoist::model::Due;
    use crate::ui::theme::Theme;

    fn sunday() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 22).unwrap()
    }

    fn monday() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 23).unwrap()
    }

    /// UTC-6, as `America/Mexico_City`.
    fn west() -> FixedOffset {
        FixedOffset::west_opt(6 * 3600).unwrap()
    }

    /// UTC+9, as `Asia/Tokyo`.
    fn east() -> FixedOffset {
        FixedOffset::east_opt(9 * 3600).unwrap()
    }

    fn due(date: &str) -> Due {
        Due {
            date: date.to_owned(),
            ..Due::default()
        }
    }

    /// A due with a fixed timezone: Todoist sends its instant in UTC.
    fn fixed(date: &str) -> Due {
        Due {
            timezone: Some("America/Mexico_City".to_owned()),
            ..due(date)
        }
    }

    #[test]
    fn a_due_string_typed_on_an_earlier_day_never_shows() {
        let theme = &Theme::built_in("nord").unwrap();
        let stale = Due {
            string: Some("tomorrow".to_owned()),
            ..due("2026-11-22")
        };

        assert_eq!(format_due(&stale, sunday(), &Utc, theme).text, "today");
    }

    #[test]
    fn a_recurring_due_shows_its_next_date_not_its_rule() {
        let theme = &Theme::built_in("nord").unwrap();
        let recurring = Due {
            string: Some("every month at 17:00".to_owned()),
            is_recurring: true,
            ..due("2026-11-28T17:00:00")
        };

        assert_eq!(
            format_due(&recurring, sunday(), &Utc, theme).text,
            "Nov 28 17:00"
        );
    }

    #[test]
    fn a_timed_due_earlier_today_reads_and_looks_like_today() {
        let theme = &Theme::built_in("nord").unwrap();

        let formatted = format_due(&due("2026-11-22T09:00:00"), sunday(), &Utc, theme);

        assert_eq!(formatted.text, "today 9:00");
        assert_eq!(formatted.style, theme.due_today());
    }

    #[test]
    fn a_timed_due_yesterday_looks_overdue() {
        let theme = &Theme::built_in("nord").unwrap();

        let formatted = format_due(&due("2026-11-21T18:30:00Z"), sunday(), &Utc, theme);

        assert_eq!(formatted.text, "yesterday");
        assert_eq!(formatted.style, theme.due_overdue());
    }

    #[test]
    fn date_part_strips_time_suffix() {
        assert_eq!(date_part("2026-06-16T09:00:00"), "2026-06-16");
        assert_eq!(date_part("2026-06-16"), "2026-06-16");
        assert_eq!(date_part(""), "");
    }

    fn bucket(date: &str) -> DueBucket {
        due_bucket(Some(&due(date)), sunday(), &Utc)
    }

    #[test]
    fn day_boundaries_for_all_day_dues() {
        assert_eq!(bucket("2026-11-21"), DueBucket::Overdue);
        assert_eq!(bucket("2026-11-22"), DueBucket::Today);
        assert_eq!(bucket("2026-11-23"), DueBucket::ThisWeek);
    }

    #[test]
    fn day_boundaries_for_timed_dues() {
        assert_eq!(bucket("2026-11-21T23:59:59"), DueBucket::Overdue);
        assert_eq!(bucket("2026-11-22T00:00:00"), DueBucket::Today);
        assert_eq!(bucket("2026-11-22T23:59:59"), DueBucket::Today);
        assert_eq!(bucket("2026-11-23T00:00:00"), DueBucket::ThisWeek);
    }

    #[test]
    fn floating_dues_keep_the_day_they_name_in_any_zone() {
        for zone in [west(), east()] {
            let on = |date: &str| due_bucket(Some(&due(date)), sunday(), &zone);
            assert_eq!(on("2026-11-22"), DueBucket::Today);
            assert_eq!(on("2026-11-22T00:00:00"), DueBucket::Today);
            assert_eq!(on("2026-11-22T23:59:59"), DueBucket::Today);
            assert_eq!(on("2026-11-23T00:00:00"), DueBucket::ThisWeek);
        }
    }

    #[test]
    fn week_boundaries_for_all_day_dues() {
        assert_eq!(bucket("2026-11-28"), DueBucket::ThisWeek);
        assert_eq!(bucket("2026-11-29"), DueBucket::Later);
    }

    #[test]
    fn week_boundaries_for_timed_dues() {
        assert_eq!(bucket("2026-11-28T23:59:59"), DueBucket::ThisWeek);
        assert_eq!(bucket("2026-11-29T00:00:00"), DueBucket::Later);
    }

    #[test]
    fn a_fixed_timezone_due_counts_by_its_day_in_the_users_zone() {
        assert_eq!(bucket("2026-11-21T23:00:00Z"), DueBucket::Overdue);
        assert_eq!(bucket("2026-11-22T08:00:00Z"), DueBucket::Today);
        let in_zone = |date: &str, zone| due_bucket(Some(&fixed(date)), sunday(), &zone);
        assert_eq!(in_zone("2026-11-21T23:00:00Z", east()), DueBucket::Today);
        assert_eq!(in_zone("2026-11-22T03:00:00Z", west()), DueBucket::Overdue);
    }

    #[test]
    fn a_fixed_due_late_tonight_west_of_utc_is_due_today() {
        let theme = &Theme::built_in("nord").unwrap();
        let tonight = fixed("2026-11-23T05:30:00Z");

        assert_eq!(
            due_bucket(Some(&tonight), sunday(), &west()),
            DueBucket::Today
        );
        let formatted = format_due(&tonight, sunday(), &west(), theme);
        assert_eq!(formatted.text, "today");
        assert_eq!(formatted.style, theme.due_today());
    }

    #[test]
    fn a_fixed_due_early_tomorrow_east_of_utc_is_due_tomorrow() {
        let theme = &Theme::built_in("nord").unwrap();
        let tomorrow_morning = fixed("2026-11-22T23:00:00Z");

        assert_eq!(
            due_bucket(Some(&tomorrow_morning), sunday(), &east()),
            DueBucket::ThisWeek
        );
        assert_eq!(
            format_due(&tomorrow_morning, sunday(), &east(), theme).text,
            "tomorrow"
        );
        assert_eq!(
            due_bucket(Some(&tomorrow_morning), monday(), &east()),
            DueBucket::Today
        );
        assert_eq!(
            format_due(&tomorrow_morning, monday(), &east(), theme).text,
            "today"
        );
    }

    #[test]
    fn a_fixed_due_turns_over_at_local_midnight() {
        let theme = &Theme::built_in("nord").unwrap();
        // 23:59 and 00:00 local, on both sides of UTC.
        for (zone, last_minute, midnight) in [
            (west(), "2026-11-23T05:59:00Z", "2026-11-23T06:00:00Z"),
            (east(), "2026-11-22T14:59:00Z", "2026-11-22T15:00:00Z"),
        ] {
            let on = |date: &str| due_bucket(Some(&fixed(date)), sunday(), &zone);
            let reads = |date: &str| format_due(&fixed(date), sunday(), &zone, theme).text;
            assert_eq!(on(last_minute), DueBucket::Today, "{zone}");
            assert_eq!(reads(last_minute), "today", "{zone}");
            assert_eq!(on(midnight), DueBucket::ThisWeek, "{zone}");
            assert_eq!(reads(midnight), "tomorrow", "{zone}");
        }
    }

    #[test]
    fn a_fixed_due_before_local_midnight_today_is_overdue() {
        let late_yesterday = fixed("2026-11-22T05:59:00Z");
        let midnight = fixed("2026-11-22T06:00:00Z");

        assert_eq!(
            due_bucket(Some(&late_yesterday), sunday(), &west()),
            DueBucket::Overdue
        );
        assert_eq!(
            due_bucket(Some(&midnight), sunday(), &west()),
            DueBucket::Today
        );
    }

    #[test]
    fn a_due_with_an_offset_counts_as_an_instant() {
        assert_eq!(
            due_bucket(Some(&due("2026-11-23T05:30:00+00:00")), sunday(), &west()),
            DueBucket::Today
        );
    }

    #[test]
    fn a_far_fixed_due_reads_as_its_local_date() {
        let theme = &Theme::built_in("nord").unwrap();
        let formatted = format_due(&fixed("2026-12-13T03:00:00Z"), sunday(), &west(), theme);
        assert_eq!(formatted.text, "Dec 12");
    }

    #[test]
    fn the_week_crosses_a_month_and_a_year() {
        let new_years_eve = NaiveDate::from_ymd_opt(2026, 12, 31).unwrap();
        let on = |date: &str| due_bucket(Some(&due(date)), new_years_eve, &Utc);
        assert_eq!(on("2026-12-30"), DueBucket::Overdue);
        assert_eq!(on("2027-01-06T10:00:00"), DueBucket::ThisWeek);
        assert_eq!(on("2027-01-07"), DueBucket::Later);
    }

    #[test]
    fn no_due_or_an_unreadable_one_has_no_date() {
        assert_eq!(due_bucket(None, sunday(), &Utc), DueBucket::NoDate);
        assert_eq!(bucket("not-a-date"), DueBucket::NoDate);
        assert_eq!(bucket("2026/11/22"), DueBucket::NoDate);
        assert_eq!(bucket(""), DueBucket::NoDate);
    }

    #[test]
    fn a_due_without_a_string_reads_as_a_short_date() {
        let theme = &Theme::built_in("nord").unwrap();
        let formatted = format_due(&due("2026-12-12T18:00:00"), sunday(), &Utc, theme);
        assert_eq!(formatted.text, "Dec 12 18:00");
        assert_eq!(formatted.style, theme.due_future());
    }
}
