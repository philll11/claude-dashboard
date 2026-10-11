//! Numbers, money, dates and ages as the design writes them.

use jiff::civil::Date;
use jiff::{Timestamp, Zoned};

/// A count with thousands separators: `2,775`.
pub fn grouped(count: u64) -> String {
    let digits = count.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// A count of tokens in a few characters: `640`, `3K`, `166K`, `1.0M`, `150M`, `11.5B`.
#[allow(clippy::cast_precision_loss)]
pub fn compact(count: u64) -> String {
    let value = count as f64;
    // Halves round away from zero, as in the design.
    let scaled = |divisor: f64, unit: &str| {
        let scaled = value / divisor;
        if scaled >= 100.0 {
            format!("{}{unit}", scaled.round())
        } else {
            format!("{:.1}{unit}", (scaled * 10.0).round() / 10.0)
        }
    };
    if count >= 999_500_000 {
        scaled(1e9, "B")
    } else if count >= 999_500 {
        scaled(1e6, "M")
    } else if count >= 1_000 {
        format!("{}K", (value / 1e3).round())
    } else {
        count.to_string()
    }
}

/// US dollars to the cent, with thousands separators: `$5.05`, `$1,380.57`.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn money(usd: f64) -> String {
    let cents = (usd * 100.0).round().max(0.0) as u64;
    format!("${}.{:02}", grouped(cents / 100), cents % 100)
}

/// A day as the design writes it: `Saturday 10 October`.
pub fn long_day(day: Date) -> String {
    format!("{} {} {}", weekday(day), day.day(), month(day))
}

/// A date with the year: `1 Oct 2026`.
pub fn short_date(day: Date) -> String {
    format!("{} {} {}", day.day(), &month(day)[..3], day.year())
}

fn weekday(day: Date) -> &'static str {
    match day.weekday() {
        jiff::civil::Weekday::Monday => "Monday",
        jiff::civil::Weekday::Tuesday => "Tuesday",
        jiff::civil::Weekday::Wednesday => "Wednesday",
        jiff::civil::Weekday::Thursday => "Thursday",
        jiff::civil::Weekday::Friday => "Friday",
        jiff::civil::Weekday::Saturday => "Saturday",
        jiff::civil::Weekday::Sunday => "Sunday",
    }
}

fn month(day: Date) -> &'static str {
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    MONTHS[usize::try_from(day.month() - 1).unwrap_or(0)]
}

/// When something happens, from `now`'s point of view: `14:20` today, `Thu 09:00` within a week,
/// and `15 Oct 09:00` after that.
pub fn when(at: &Zoned, now: &Zoned) -> String {
    let time = format!("{:02}:{:02}", at.hour(), at.minute());
    let days = (at.date() - now.date()).get_days();
    if days == 0 {
        time
    } else if (1..7).contains(&days) {
        format!("{} {time}", &weekday(at.date())[..3])
    } else {
        format!("{} {} {time}", at.day(), &month(at.date())[..3])
    }
}

/// How long ago something happened: `just now`, `2 min ago`, `3 h ago`, `4 days ago`.
pub fn ago(then: Timestamp, now: Timestamp) -> String {
    let minutes = (now.as_second() - then.as_second()).max(0) / 60;
    match minutes {
        0 => "just now".to_owned(),
        1..60 => format!("{minutes} min ago"),
        60..1440 => format!("{} h ago", minutes / 60),
        _ => {
            let days = minutes / 1440;
            format!("{days} {} ago", if days == 1 { "day" } else { "days" })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    /// The examples every surface's formatters are checked against.
    fn examples() -> Value {
        serde_json::from_str(include_str!("../../../design/formats.json")).unwrap()
    }

    fn pairs<'a>(examples: &'a Value, key: &str) -> &'a Vec<Value> {
        examples[key].as_array().unwrap()
    }

    #[test]
    fn counts_read_naturally() {
        for pair in pairs(&examples(), "grouped") {
            assert_eq!(grouped(pair[0].as_u64().unwrap()), pair[1], "{pair}");
        }
    }

    #[test]
    fn token_counts_are_compact_as_the_design_writes_them() {
        for pair in pairs(&examples(), "compact") {
            assert_eq!(compact(pair[0].as_u64().unwrap()), pair[1], "{pair}");
        }
    }

    #[test]
    fn money_is_to_the_cent() {
        for pair in pairs(&examples(), "money") {
            assert_eq!(money(pair[0].as_f64().unwrap()), pair[1], "{pair}");
        }
    }

    #[test]
    fn days_and_times_read_as_in_the_design() {
        let examples = examples();
        let zone = |section: &Value| {
            jiff::tz::TimeZone::get(section["timeZone"].as_str().unwrap()).unwrap()
        };
        let days = &examples["days"];
        for example in days["examples"].as_array().unwrap() {
            let day: Date = example["day"].as_str().unwrap().parse().unwrap();
            assert_eq!(long_day(day), example["long"], "{example}");
            assert_eq!(short_date(day), example["short"], "{example}");
        }
        let times = &examples["when"];
        let at = |ms: &Value| {
            Timestamp::from_millisecond(ms.as_i64().unwrap())
                .unwrap()
                .to_zoned(zone(times))
        };
        let now = at(&times["nowMs"]);
        for pair in times["examples"].as_array().unwrap() {
            assert_eq!(when(&at(&pair[0]), &now), pair[1], "{pair}");
        }
    }

    #[test]
    fn ages_read_naturally() {
        let ages = &examples()["ago"];
        let now = Timestamp::from_millisecond(ages["nowMs"].as_i64().unwrap()).unwrap();
        for pair in ages["examples"].as_array().unwrap() {
            let then = Timestamp::from_second(now.as_second() - pair[0].as_i64().unwrap()).unwrap();
            assert_eq!(ago(then, now), pair[1], "{pair}");
        }
    }
}
