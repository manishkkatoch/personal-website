//! iCalendar (RFC 5545) output for single events and the subscribable feed.

use crate::{
    content::{Content, Event},
    date, markdown,
};

pub fn feed(c: &Content, now: i64, site_url: &str) -> String {
    calendar(c, c.events.iter(), now, site_url)
}

pub fn single(c: &Content, e: &Event, now: i64, site_url: &str) -> String {
    calendar(c, std::iter::once(e), now, site_url)
}

fn calendar<'a>(c: &Content, events: impl Iterator<Item = &'a Event>, now: i64, site_url: &str) -> String {
    let host = site_url.split("://").nth(1).unwrap_or("field").split('/').next().unwrap_or("field");
    let mut lines = vec![
        "BEGIN:VCALENDAR".to_string(),
        "VERSION:2.0".into(),
        format!("PRODID:-//{}//field//EN", text(&c.site.name)),
        "CALSCALE:GREGORIAN".into(),
        "METHOD:PUBLISH".into(),
        format!("X-WR-CALNAME:{}", text(&c.site.name)),
    ];
    for e in events {
        lines.push("BEGIN:VEVENT".into());
        lines.push(format!("UID:{}@{}", e.slug, host));
        lines.push(format!("DTSTAMP:{}", date::ics(now)));
        lines.push(format!("DTSTART:{}", date::ics(e.start)));
        lines.push(format!("DTEND:{}", date::ics(e.end)));
        lines.push(format!("SUMMARY:{}", text(&e.title)));
        if !e.location.is_empty() {
            lines.push(format!("LOCATION:{}", text(&e.location)));
        }
        if !e.url.is_empty() {
            lines.push(format!("URL:{}", e.url));
        }
        let body = markdown::plain(&e.body);
        if !body.is_empty() {
            lines.push(format!("DESCRIPTION:{}", text(&body)));
        }
        lines.push("END:VEVENT".into());
    }
    lines.push("END:VCALENDAR".into());
    lines.iter().map(|l| fold(l)).collect::<Vec<_>>().join("")
}

fn text(s: &str) -> String {
    s.replace('\\', "\\\\").replace(';', "\\;").replace(',', "\\,").replace('\n', "\\n")
}

/// Folds a content line at 75 octets, never inside a UTF-8 sequence.
fn fold(line: &str) -> String {
    let mut out = String::with_capacity(line.len() + 8);
    let mut width = 0;
    for c in line.chars() {
        if width + c.len_utf8() > 75 {
            out.push_str("\r\n ");
            width = 1;
        }
        out.push(c);
        width += c.len_utf8();
    }
    out.push_str("\r\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::load;

    #[test]
    fn writes_a_valid_looking_calendar() {
        let c = load(&[
            ("site.md", "---\nname: Ada\n---\n"),
            ("events/talk.md", "---\ntitle: Fields, waves; and points\nstart: 2026-11-14T18:30+01:00\nlocation: Online\n---\nA long description that should be folded because it goes on and on beyond seventy five octets."),
        ]);
        let out = feed(&c, 0, "https://ada.example/");
        assert!(out.contains("DTSTART:20261114T173000Z\r\n"));
        assert!(out.contains("SUMMARY:Fields\\, waves\\; and points\r\n"));
        assert!(out.contains("UID:talk@ada.example\r\n"));
        assert!(out.lines().all(|l| l.len() <= 76));
        assert!(out.ends_with("END:VCALENDAR\r\n"));
    }
}
