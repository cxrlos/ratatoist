//! The Upcoming screen: open tasks due in the next seven days in the rows
//! Today has, grouped by day under a header such as `Mon 23 Nov (3)`.

use jiff::civil::Date;

/// What the header of `day`'s group calls it, as `Mon 23 Nov`.
pub fn day_label(day: Option<Date>) -> String {
    day.map_or_else(
        || "No date".to_owned(),
        |date| date.strftime("%a %-d %b").to_string(),
    )
}
