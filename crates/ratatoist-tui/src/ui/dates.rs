use ratatui::style::Style;

use super::theme::Theme;

pub struct FormattedDue {
    pub text: String,
    pub style: Style,
}

pub fn format_due(date_str: &str, theme: &Theme) -> FormattedDue {
    let today = today_str();
    let tomorrow = offset_days_str(1);
    let yesterday = offset_days_str(-1);

    if date_str == today {
        return FormattedDue {
            text: "today".to_string(),
            style: theme.due_today(),
        };
    }

    if date_str == tomorrow {
        return FormattedDue {
            text: "tomorrow".to_string(),
            style: theme.due_upcoming(),
        };
    }

    if date_str == yesterday {
        return FormattedDue {
            text: "yesterday".to_string(),
            style: theme.due_overdue(),
        };
    }

    if date_str < today.as_str() {
        return FormattedDue {
            text: format_short_date(date_str),
            style: theme.due_overdue(),
        };
    }

    let days_away = days_between(&today, date_str);
    if days_away <= 6 {
        return FormattedDue {
            text: weekday_name(date_str),
            style: theme.due_upcoming(),
        };
    }

    FormattedDue {
        text: format_short_date(date_str),
        style: theme.due_future(),
    }
}

pub fn today_str() -> String {
    chrono::Local::now()
        .date_naive()
        .format("%Y-%m-%d")
        .to_string()
}

pub fn offset_days_str(days: i64) -> String {
    let today = chrono::Local::now().date_naive();
    (today + chrono::Duration::days(days))
        .format("%Y-%m-%d")
        .to_string()
}

fn parse_date(s: &str) -> Option<(i32, u32, u32)> {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    Some((
        parts[0].parse().ok()?,
        parts[1].parse().ok()?,
        parts[2].parse().ok()?,
    ))
}

fn days_between(a: &str, b: &str) -> i64 {
    let da = parse_date(a).map(|(y, m, d)| days_from_civil(y, m, d));
    let db = parse_date(b).map(|(y, m, d)| days_from_civil(y, m, d));
    match (da, db) {
        (Some(a), Some(b)) => b - a,
        _ => 999,
    }
}

fn weekday_name(date_str: &str) -> String {
    let Some((y, m, d)) = parse_date(date_str) else {
        return date_str.to_string();
    };
    let days = days_from_civil(y, m, d);
    let weekday = ((days % 7) + 4) % 7; // 0=Sun
    let names = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    names[weekday as usize % 7].to_string()
}

fn format_short_date(date_str: &str) -> String {
    let Some((_, m, d)) = parse_date(date_str) else {
        return date_str.to_string();
    };
    let months = [
        "", "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let month = months.get(m as usize).unwrap_or(&"???");
    format!("{month} {d}")
}

fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y as i64 - 1 } else { y as i64 };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400) as u64;
    let m = m as u64;
    let d = d as u64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe as i64 - 719468
}
