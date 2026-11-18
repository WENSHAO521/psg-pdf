// ── built-in functions ──────────────────────────────────────────────────────────────────────

use jiff::{SignedDuration, civil::Date};

/// FormCalc counts 1900-01-01 as day one.
const FORMCALC_EPOCH: Date = Date::constant(1900, 1, 1);

/// Days in one 400-year Gregorian cycle; the calendar repeats after it.
const DAYS_PER_400_YEARS: i64 = 146_097;

/// Days from 1900-01-01 (day 1, as FormCalc counts) to a civil date.
///
/// FormCalc accepts day numbers beyond Jiff's range. The calendar repeats every 400 years,
/// so Jiff converts the date
/// within its cycle and whole cycles are added back; the result matches the proleptic Gregorian
/// count for every year. Callers pass dates `parse_date`/`civil_from_days` already validated;
/// anything else reads as 0.
pub(super) fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let (Ok(month), Ok(day)) = (i8::try_from(m), i8::try_from(d)) else { return 0 };
    let Ok(date) = Date::new(y.rem_euclid(400) as i16, month, day) else { return 0 };
    let local_days = date.duration_since(FORMCALC_EPOCH).as_secs() / 86_400 + 1;
    let cycles = i128::from(y.div_euclid(400));
    i64::try_from(i128::from(local_days) + cycles * i128::from(DAYS_PER_400_YEARS)).unwrap_or(0)
}

/// The civil date (year, month, day) at FormCalc day number `n` (day 1 is 1900-01-01), for any
/// `n`: the calendar library converts the day within one 400-year cycle.
pub(super) fn civil_from_days(n: i64) -> (i64, u32, u32) {
    let days = i128::from(n) - 1;
    let (cycles, rest) = (days.div_euclid(i128::from(DAYS_PER_400_YEARS)), days.rem_euclid(i128::from(DAYS_PER_400_YEARS)));
    match FORMCALC_EPOCH.checked_add(SignedDuration::from_hours(rest as i64 * 24)) {
        Ok(date) => {
            let y = i128::from(date.year()) + cycles * 400;
            (i64::try_from(y).unwrap_or(if y < 0 { i64::MIN } else { i64::MAX }), date.month() as u32, date.day() as u32)
        }
        Err(_) => (1900, 1, 1), // unreachable: the remainder is within 400 years of 1900
    }
}

/// Day numbers (and years) beyond this are refused rather than computed.
pub(super) const MAX_DAYS: f64 = 1e8;

/// The century split of a two-digit year (`YY`), XFA picture clauses' default: `00` to `29` are
/// 2000 to 2029 and `30` to `99` are 1930 to 1999 (XFA 3.3, chapter 26, Picture Clause Reference,
/// p. 1171, which makes 30 the default and lets the century split be set otherwise).
const CENTURY_SPLIT: i64 = 30;

const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

/// Parse `text` as a date by `picture` (`YYYY-MM-DD`, `MM/DD/YYYY`, `MMM D, YYYY`, …).
pub(super) fn parse_date(text: &str, picture: &str) -> Option<(i64, u32, u32)> {
    let pic: Vec<char> = picture.chars().collect();
    let txt: Vec<char> = text.trim().chars().collect();
    let (mut y, mut m, mut d) = (None, None, None);
    let (mut i, mut j) = (0, 0);
    while i < pic.len() {
        let c = pic[i].to_ascii_uppercase();
        if c == 'Y' || c == 'M' || c == 'D' {
            let mut run = 1;
            while i + run < pic.len() && pic[i + run].to_ascii_uppercase() == c {
                run += 1;
            }
            i += run;
            if c == 'M' && run >= 3 {
                // Month name.
                let rest: String = txt.get(j..).unwrap_or(&[]).iter().collect();
                let lower = rest.to_ascii_lowercase();
                let found = MONTHS
                    .iter()
                    .position(|name| lower.starts_with(&name.to_ascii_lowercase()) || lower.starts_with(&name[..3].to_ascii_lowercase()));
                let k = found?;
                m = Some(k as u32 + 1);
                let full = MONTHS[k].to_ascii_lowercase();
                j += if lower.starts_with(&full) { full.chars().count() } else { 3 };
                continue;
            }
            let width = if run == 1 { 2 } else { run };
            let mut digits = String::new();
            while j < txt.len() && txt[j].is_ascii_digit() && digits.len() < width {
                digits.push(txt[j]);
                j += 1;
            }
            if digits.is_empty() {
                return None;
            }
            let v: i64 = digits.parse().ok()?;
            match c {
                'Y' => y = Some(if run <= 2 { if v < CENTURY_SPLIT { 2000 + v } else { 1900 + v } } else { v }),
                'M' => m = Some(v as u32),
                _ => d = Some(v as u32),
            }
        } else {
            if txt.get(j) != Some(&pic[i]) {
                return None;
            }
            i += 1;
            j += 1;
        }
    }
    let (y, m, d) = (y?, m.unwrap_or(1), d.unwrap_or(1));
    ((-200_000..=200_000).contains(&y) && (1..=12).contains(&m) && (1..=days_in(y, m)).contains(&d)).then_some((y, m, d))
}

/// Number of days in month `m` (1-12) of year `y`, leap years included.
pub(crate) fn days_in(y: i64, m: u32) -> u32 {
    // Month lengths repeat every 400 years. An invalid month keeps the old 31-day default.
    i8::try_from(m).ok().and_then(|m| Date::new(y.rem_euclid(400) as i16, m, 1).ok()).map_or(31, |d| d.days_in_month() as u32)
}

const WEEKDAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

pub(super) fn format_date(y: i64, m: u32, d: u32, picture: &str) -> String {
    let pic: Vec<char> = picture.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < pic.len() {
        let c = pic[i].to_ascii_uppercase();
        if c == 'Y' || c == 'M' || c == 'D' {
            let mut run = 1;
            while i + run < pic.len() && pic[i + run].to_ascii_uppercase() == c {
                run += 1;
            }
            i += run;
            match (c, run) {
                ('Y', 1 | 2) => out.push_str(&format!("{:02}", y.rem_euclid(100))),
                ('Y', _) => out.push_str(&format!("{y:04}")),
                ('M', 1) => out.push_str(&m.to_string()),
                ('M', 2) => out.push_str(&format!("{m:02}")),
                ('M', 3) => out.push_str(MONTHS.get((m as usize).saturating_sub(1)).map_or("", |n| &n[..3])),
                ('M', _) => out.push_str(MONTHS.get((m as usize).saturating_sub(1)).copied().unwrap_or("")),
                ('D', 1) => out.push_str(&d.to_string()),
                (_, _) => out.push_str(&format!("{d:02}")),
            }
        } else if pic[i] == 'E' {
            let mut run = 1;
            while i + run < pic.len() && pic[i + run] == 'E' {
                run += 1;
            }
            i += run;
            // Day 1 (1900-01-01) was a Monday, so the serial modulo 7 indexes from Sunday.
            let weekday = days_from_civil(y, m, d).rem_euclid(7) as usize;
            match run {
                1 | 2 => out.push_str(&(weekday + 1).to_string()),
                3 => out.push_str(WEEKDAYS.get(weekday).map_or("", |n| &n[..3])),
                _ => out.push_str(WEEKDAYS.get(weekday).copied().unwrap_or("")),
            }
        } else {
            out.push(pic[i]);
            i += 1;
        }
    }
    out
}
