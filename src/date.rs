//! Calendar arithmetic on unix seconds. No time zone database: content
//! carries explicit UTC offsets, and the browser localises for visitors.

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

pub const DAY: i64 = 86_400;

/// Days since 1970-01-01 for a proleptic Gregorian date.
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let (m, d) = (m as i64, d as i64);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Inverse of [`days_from_civil`].
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// Monday = 0.
pub fn weekday(days: i64) -> u32 {
    (days + 3).rem_euclid(7) as u32
}

pub fn days_in_month(y: i64, m: u32) -> u32 {
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    (days_from_civil(ny, nm, 1) - days_from_civil(y, m, 1)) as u32
}

/// A parsed timestamp plus the UTC offset (minutes) it was written in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stamp {
    pub ts: i64,
    pub offset: i32,
}

/// Parses `YYYY-MM-DD`, `YYYY-MM-DDTHH:MM[:SS]`, optionally followed by
/// `Z`, `+HH:MM` or `-HHMM`. A missing offset means UTC.
pub fn parse(s: &str) -> Option<Stamp> {
    let s = s.trim();
    let b = s.as_bytes();
    if b.len() < 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let y: i64 = s.get(0..4)?.parse().ok()?;
    let m: u32 = s.get(5..7)?.parse().ok()?;
    let d: u32 = s.get(8..10)?.parse().ok()?;
    if !(1..=12).contains(&m) || d == 0 || d > days_in_month(y, m) {
        return None;
    }
    let mut rest = s.get(10..)?;
    let (mut hh, mut mm, mut ss) = (0i64, 0i64, 0i64);
    if rest.starts_with('T') || rest.starts_with(' ') {
        rest = &rest[1..];
        hh = rest.get(0..2)?.parse().ok()?;
        if rest.as_bytes().get(2) != Some(&b':') {
            return None;
        }
        mm = rest.get(3..5)?.parse().ok()?;
        rest = &rest[5..];
        if let Some(r) = rest.strip_prefix(':') {
            ss = r.get(0..2)?.parse().ok()?;
            rest = &r[2..];
        }
    }
    let rest = rest.trim();
    let offset = match rest.as_bytes().first() {
        None | Some(b'Z') => 0,
        Some(&sign @ (b'+' | b'-')) => {
            let digits: String = rest[1..].chars().filter(|c| c.is_ascii_digit()).collect();
            if digits.len() != 4 {
                return None;
            }
            let v = digits[0..2].parse::<i32>().ok()? * 60 + digits[2..4].parse::<i32>().ok()?;
            if sign == b'-' {
                -v
            } else {
                v
            }
        }
        _ => return None,
    };
    let ts = days_from_civil(y, m, d) * DAY + hh * 3600 + mm * 60 + ss - offset as i64 * 60;
    Some(Stamp { ts, offset })
}

pub struct Civil {
    pub y: i64,
    pub m: u32,
    pub d: u32,
    pub hh: u32,
    pub mm: u32,
    pub ss: u32,
    pub wd: u32,
}

/// Wall-clock fields for `ts` at a UTC offset in minutes.
pub fn civil(ts: i64, offset: i32) -> Civil {
    let t = ts + offset as i64 * 60;
    let days = t.div_euclid(DAY);
    let secs = t.rem_euclid(DAY) as u32;
    let (y, m, d) = civil_from_days(days);
    Civil { y, m, d, hh: secs / 3600, mm: secs / 60 % 60, ss: secs % 60, wd: weekday(days) }
}

/// `12 Jan 2026`
pub fn long(ts: i64) -> String {
    let c = civil(ts, 0);
    format!("{} {} {}", c.d, MONTHS[c.m as usize - 1], c.y)
}

/// `2026-01-12`
pub fn iso_date(ts: i64) -> String {
    let c = civil(ts, 0);
    format!("{:04}-{:02}-{:02}", c.y, c.m, c.d)
}

/// `2026-01-12T17:30:00Z`
pub fn iso(ts: i64) -> String {
    let c = civil(ts, 0);
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", c.y, c.m, c.d, c.hh, c.mm, c.ss)
}

/// `20260112T173000Z`, as calendar files want it.
pub fn ics(ts: i64) -> String {
    let c = civil(ts, 0);
    format!("{:04}{:02}{:02}T{:02}{:02}{:02}Z", c.y, c.m, c.d, c.hh, c.mm, c.ss)
}

/// `Mon, 12 Jan 2026 17:30:00 +0000`, as RSS wants it.
pub fn rfc2822(ts: i64) -> String {
    let c = civil(ts, 0);
    format!(
        "{}, {:02} {} {} {:02}:{:02}:{:02} +0000",
        WEEKDAYS[c.wd as usize],
        c.d,
        MONTHS[c.m as usize - 1],
        c.y,
        c.hh,
        c.mm,
        c.ss
    )
}

/// `Sat 14 Nov 2026, 18:30 UTC+1` in the offset the event was written in.
pub fn event(ts: i64, offset: i32) -> String {
    let c = civil(ts, offset);
    let zone = match offset {
        0 => "UTC".to_string(),
        o if o % 60 == 0 => format!("UTC{:+}", o / 60),
        o => format!("UTC{}{:02}:{:02}", if o < 0 { '-' } else { '+' }, o.abs() / 60, o.abs() % 60),
    };
    format!(
        "{} {} {} {}, {:02}:{:02} {}",
        WEEKDAYS[c.wd as usize],
        c.d,
        MONTHS[c.m as usize - 1],
        c.y,
        c.hh,
        c.mm,
        zone
    )
}

/// Distance from `now` to `ts` in calendar days, in words.
pub fn relative(ts: i64, now: i64) -> String {
    let d = ts.div_euclid(DAY) - now.div_euclid(DAY);
    let n = d.abs();
    let (amount, unit) = match n {
        0 => return "today".into(),
        1 => return if d > 0 { "tomorrow".into() } else { "yesterday".into() },
        2..=13 => (n, "day"),
        14..=59 => (n / 7, "week"),
        60..=729 => (n / 30, "month"),
        _ => (n / 365, "year"),
    };
    let unit = if amount == 1 { unit.to_string() } else { format!("{unit}s") };
    if d > 0 {
        format!("in {amount} {unit}")
    } else {
        format!("{amount} {unit} ago")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_civil_dates() {
        for days in [-800_000, -1, 0, 1, 19_000, 20_000, 2_000_000] {
            let (y, m, d) = civil_from_days(days);
            assert_eq!(days_from_civil(y, m, d), days);
        }
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(weekday(0), 3); // a Thursday
    }

    #[test]
    fn parses_offsets() {
        let a = parse("2026-11-14T18:30+01:00").unwrap();
        let b = parse("2026-11-14T17:30Z").unwrap();
        assert_eq!(a.ts, b.ts);
        assert_eq!(a.offset, 60);
        assert_eq!(parse("2026-11-14T12:00-0530").unwrap().offset, -330);
        assert_eq!(parse("2026-02-30"), None);
        assert_eq!(event(a.ts, a.offset), "Sat 14 Nov 2026, 18:30 UTC+1");
    }

    #[test]
    fn words_for_distance() {
        let now = parse("2026-10-22T12:00Z").unwrap().ts;
        assert_eq!(relative(now + DAY, now), "tomorrow");
        assert_eq!(relative(now - 3 * DAY, now), "3 days ago");
        assert_eq!(relative(now + 23 * DAY, now), "in 3 weeks");
        assert_eq!(relative(now - 400 * DAY, now), "13 months ago");
    }
}
