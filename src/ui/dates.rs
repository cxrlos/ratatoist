use chrono::NaiveDate;
use ratatui::style::Style;

use super::theme::Theme;
use crate::todoist::model::Due;

pub struct FormattedDue {
    pub text: String,
    pub style: Style,
}

/// Where a due date falls relative to today. Timed dues count by their
/// calendar day, so a task due at 09:00 today is due today all day.
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

/// The bucket `due` falls in on `today`.
pub fn due_bucket(due: Option<&Due>, today: NaiveDate) -> DueBucket {
    match due.and_then(|due| days_away(due, today)) {
        None => DueBucket::NoDate,
        Some(days) if days < 0 => DueBucket::Overdue,
        Some(0) => DueBucket::Today,
        Some(1..=6) => DueBucket::ThisWeek,
        Some(_) => DueBucket::Later,
    }
}

/// Days from `today` to the calendar day of `due`, negative when past.
fn days_away(due: &Due, today: NaiveDate) -> Option<i64> {
    let day = NaiveDate::parse_from_str(date_part(&due.date), "%Y-%m-%d").ok()?;
    Some((day - today).num_days())
}

/// How `due` reads and looks on `today`.
pub fn format_due(due: &Due, today: NaiveDate, theme: &Theme) -> FormattedDue {
    let style = match due_bucket(Some(due), today) {
        DueBucket::Overdue => theme.due_overdue(),
        DueBucket::Today => theme.due_today(),
        DueBucket::ThisWeek => theme.due_upcoming(),
        DueBucket::Later | DueBucket::NoDate => theme.due_future(),
    };
    FormattedDue {
        text: display_label(due, days_away(due, today)),
        style,
    }
}

fn display_label(due: &Due, days_away: Option<i64>) -> String {
    if let Some(s) = &due.string
        && !s.is_empty()
    {
        return s.clone();
    }

    match days_away {
        Some(0) => "today".to_string(),
        Some(1) => "tomorrow".to_string(),
        Some(-1) => "yesterday".to_string(),
        _ => format_short_date(date_part(&due.date)),
    }
}

/// The calendar-date portion of a Todoist due date, which may be a bare
/// `YYYY-MM-DD` or a full `YYYY-MM-DDTHH:MM:SS` timestamp.
pub fn date_part(due_date: &str) -> &str {
    due_date.split('T').next().unwrap_or(due_date)
}

fn format_short_date(date_str: &str) -> String {
    match NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
        Ok(day) => day.format("%b %-d").to_string(),
        Err(_) => date_str.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::{DueBucket, date_part, due_bucket, format_due};
    use crate::todoist::model::Due;
    use crate::ui::theme::Theme;

    fn sunday() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 22).unwrap()
    }

    fn due(date: &str) -> Due {
        Due {
            date: date.to_owned(),
            ..Due::default()
        }
    }

    #[test]
    fn a_timed_due_earlier_today_reads_and_looks_like_today() {
        let theme = &Theme::builtin()[0];

        let formatted = format_due(&due("2026-11-22T09:00:00"), sunday(), theme);

        assert_eq!(formatted.text, "today");
        assert_eq!(formatted.style, theme.due_today());
    }

    #[test]
    fn a_timed_due_yesterday_looks_overdue() {
        let theme = &Theme::builtin()[0];

        let formatted = format_due(&due("2026-11-21T18:30:00Z"), sunday(), theme);

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
        due_bucket(Some(&due(date)), sunday())
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
    fn a_fixed_timezone_due_counts_by_the_day_it_names() {
        assert_eq!(bucket("2026-11-21T23:00:00Z"), DueBucket::Overdue);
        assert_eq!(bucket("2026-11-22T08:00:00Z"), DueBucket::Today);
    }

    #[test]
    fn the_week_crosses_a_month_and_a_year() {
        let new_years_eve = NaiveDate::from_ymd_opt(2026, 12, 31).unwrap();
        let on = |date: &str| due_bucket(Some(&due(date)), new_years_eve);
        assert_eq!(on("2026-12-30"), DueBucket::Overdue);
        assert_eq!(on("2027-01-06T10:00:00"), DueBucket::ThisWeek);
        assert_eq!(on("2027-01-07"), DueBucket::Later);
    }

    #[test]
    fn no_due_or_an_unreadable_one_has_no_date() {
        assert_eq!(due_bucket(None, sunday()), DueBucket::NoDate);
        assert_eq!(bucket("not-a-date"), DueBucket::NoDate);
        assert_eq!(bucket("2026/11/22"), DueBucket::NoDate);
        assert_eq!(bucket(""), DueBucket::NoDate);
    }

    #[test]
    fn a_due_without_a_string_reads_as_a_short_date() {
        let theme = &Theme::builtin()[0];
        let formatted = format_due(&due("2026-12-12T18:00:00"), sunday(), theme);
        assert_eq!(formatted.text, "Dec 12");
        assert_eq!(formatted.style, theme.due_future());
    }
}
