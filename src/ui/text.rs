//! Measuring and cutting text for the terminal. Every width here is in
//! terminal cells, so wide characters (CJK, most emoji) count as two. Text
//! cut to fit ends in `…`, which counts toward the width it fits, with the
//! whitespace before the cut dropped.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// What marks text that was cut short.
pub const ELLIPSIS: char = '…';

/// How many terminal cells `text` takes.
pub fn width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

/// `text` as it fits in `room` cells: whole when it fits, otherwise cut
/// and ending in `…` after the last non-blank character, never wider than
/// `room`.
pub fn fit(text: &str, room: usize) -> String {
    if width(text) <= room {
        return text.to_owned();
    }
    if room == 0 {
        return String::new();
    }
    let mut cut = take(text, room - 1);
    cut.truncate(cut.trim_end().len());
    cut.push(ELLIPSIS);
    cut
}

/// `text` as it fits in `room` cells keeping its end, for text whose last
/// characters matter most (what was just typed): whole when it fits,
/// otherwise `…` and then the end of `text` after the first non-blank
/// character, never wider than `room`.
pub fn fit_start(text: &str, room: usize) -> String {
    if width(text) <= room {
        return text.to_owned();
    }
    if room == 0 {
        return String::new();
    }
    let mut used = 0;
    let kept: Vec<char> = text
        .chars()
        .rev()
        .take_while(|c| {
            used += c.width().unwrap_or(0);
            used < room
        })
        .collect();
    let end: String = kept.into_iter().rev().collect();
    format!("{ELLIPSIS}{}", end.trim_start())
}

/// `text` split after its longest start that takes at most `room` cells,
/// for breaking a run too wide for one line across several. The start is
/// empty when even the first character is wider than `room`.
pub fn split(text: &str, room: usize) -> (&str, &str) {
    let mut used = 0;
    let end = text
        .char_indices()
        .find(|(_, c)| {
            used += c.width().unwrap_or(0);
            used > room
        })
        .map_or(text.len(), |(index, _)| index);
    text.split_at(end)
}

/// `line` as it fits in `room` cells: whole when it fits, otherwise its
/// spans up to the cut, the last one ending in `…` in that span's style.
pub fn fit_line(line: Line<'_>, room: usize) -> Line<'_> {
    if line.width() <= room {
        return line;
    }
    let Line {
        spans,
        style,
        alignment,
    } = line;
    let mut kept: Vec<Span<'_>> = Vec::new();
    let mut used = 0;
    for span in spans {
        // One cell stays free for the ellipsis.
        let left = room.saturating_sub(used + 1);
        let span_width = span.width();
        if span_width <= left {
            used += span_width;
            kept.push(span);
            continue;
        }
        let cut = take(&span.content, left);
        if !cut.is_empty() {
            kept.push(Span::styled(cut, span.style));
        }
        trim_end(&mut kept);
        if room > 0 {
            kept.push(Span::styled(ELLIPSIS.to_string(), span.style));
        }
        break;
    }
    Line {
        spans: kept,
        style,
        alignment,
    }
}

/// Draws `line` on the first row of `area`, cut to its width with `…`.
pub fn draw<'a>(frame: &mut Frame, line: impl Into<Line<'a>>, area: Rect) {
    frame.render_widget(fit_line(line.into(), usize::from(area.width)), area);
}

/// Drops the whitespace that ends `spans`, so a cut reads "1…", not "1 …".
fn trim_end(spans: &mut Vec<Span<'_>>) {
    while let Some(last) = spans.last_mut() {
        let kept = last.content.trim_end().len();
        if kept > 0 {
            if kept < last.content.len() {
                last.content = last.content[..kept].to_owned().into();
            }
            return;
        }
        spans.pop();
    }
}

/// The longest start of `text` that takes at most `room` cells.
fn take(text: &str, room: usize) -> String {
    let mut used = 0;
    text.chars()
        .take_while(|c| {
            used += c.width().unwrap_or(0);
            used <= room
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Stylize;

    #[test]
    fn split_keeps_the_start_that_fits_in_cells() {
        assert_eq!(split("cheddar", 4), ("ched", "dar"));
        assert_eq!(split("cheddar", 9), ("cheddar", ""));
        assert_eq!(split("チーズ", 3), ("チ", "ーズ"));
        assert_eq!(split("チーズ", 1), ("", "チーズ"));
    }

    #[test]
    fn text_that_fits_keeps_its_start() {
        assert_eq!(fit_start("cheddar", 7), "cheddar");
    }

    #[test]
    fn text_cut_at_the_start_keeps_its_end_after_an_ellipsis() {
        assert_eq!(fit_start("cheddar", 5), "…ddar");
        assert_eq!(width(&fit_start("cheddar", 5)), 5);
        assert_eq!(fit_start("cheddar", 1), "…");
        assert_eq!(fit_start("cheddar", 0), "");
    }

    #[test]
    fn a_cut_start_counts_wide_characters_as_two_cells() {
        assert_eq!(fit_start("チーズ", 4), "…ズ");
        assert_eq!(fit_start("チーズ", 5), "…ーズ");
        assert_eq!(width(&fit_start("チーズ", 4)), 3);
    }

    #[test]
    fn a_cut_start_drops_the_space_after_the_ellipsis() {
        assert_eq!(fit_start("one two", 5), "…two");
    }

    #[test]
    fn text_that_fits_stays_whole() {
        assert_eq!(fit("cheddar", 7), "cheddar");
    }

    #[test]
    fn cut_text_ends_in_an_ellipsis_within_the_room() {
        assert_eq!(fit("cheddar", 5), "ched…");
        assert_eq!(width(&fit("cheddar", 5)), 5);
        assert_eq!(fit("cheddar", 1), "…");
        assert_eq!(fit("cheddar", 0), "");
    }

    #[test]
    fn wide_characters_count_as_two_cells() {
        assert_eq!(width("チーズ"), 6);
        assert_eq!(fit("チーズ", 4), "チ…");
        assert_eq!(fit("チーズ", 5), "チー…");
    }

    #[test]
    fn a_cut_line_keeps_whole_spans_and_ends_the_cut_one_in_its_style() {
        let line = Line::from(vec!["ENG-1 ".into(), "Grate the cheddar".red()]);

        let fitted = fit_line(line, 12);

        assert_eq!(fitted.width(), 12);
        assert_eq!(fitted.spans[0].content, "ENG-1 ");
        assert_eq!(fitted.spans[1].content, "Grate");
        assert_eq!(fitted.spans[2], "…".red());
    }

    #[test]
    fn a_cut_drops_the_trailing_space_before_the_ellipsis() {
        assert_eq!(fit("❤️ 1  👍 2", 7), "❤️ 1…");
        assert_eq!(fit("one two", 5), "one…");
    }

    #[test]
    fn a_line_cut_after_a_space_drops_it_across_spans() {
        let line = Line::from(vec!["abc".into(), "  ".red(), "def".into()]);

        let fitted = fit_line(line, 6);

        assert_eq!(fitted.to_string(), "abc…");
        assert_eq!(fitted.spans.last(), Some(&"…".into()));
    }

    #[test]
    fn a_line_cut_inside_a_spaced_span_trims_that_span() {
        let line = Line::from(vec!["ENG-1 ".into(), "Grate the cheddar".red()]);

        assert_eq!(fit_line(line, 13).to_string(), "ENG-1 Grate…");
    }

    #[test]
    fn a_line_cut_at_a_span_boundary_still_ends_in_an_ellipsis() {
        let line = Line::from(vec!["abc".into(), "def".into()]);

        assert_eq!(fit_line(line, 4).to_string(), "abc…");
    }
}
