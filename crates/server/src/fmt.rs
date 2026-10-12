//! Numbers, money, dates and ages as the design writes them.

use jiff::civil::{Date, Weekday};
use jiff::{Timestamp, Zoned};

/// A count with thousands separators: `2,775`.
#[must_use]
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
#[must_use]
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
#[must_use]
pub fn money(usd: f64) -> String {
    let cents = (usd * 100.0).round().max(0.0) as u64;
    format!("${}.{:02}", grouped(cents / 100), cents % 100)
}

/// A price in US dollars per million tokens, to the cent or to a tenth of a cent: `$12.50`,
/// `$0.125`.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
#[must_use]
pub fn price(usd: f64) -> String {
    let tenths = (usd * 1000.0).round().max(0.0) as u64;
    if tenths.is_multiple_of(10) {
        money(usd)
    } else {
        format!("${}.{:03}", grouped(tenths / 1000), tenths % 1000)
    }
}

/// A share as a whole percentage: `42%`.
#[allow(clippy::cast_possible_truncation)]
#[must_use]
pub fn percent(share: f64) -> String {
    format!("{}%", share.round() as i64)
}

/// A length of time: `45 min`, `2 h 05 min`, `4 days`.
#[must_use]
pub fn duration(ms: i64) -> String {
    // Halves round up, as in the web dashboard.
    let minutes = ((ms.max(0) + 30_000) / 60_000).max(0);
    match minutes {
        0..60 => format!("{minutes} min"),
        60..1440 => format!("{} h {:02} min", minutes / 60, minutes % 60),
        _ => {
            let days = (minutes + 720) / 1440;
            format!("{days} {}", if days == 1 { "day" } else { "days" })
        }
    }
}

/// A day as the design writes it: `Saturday 10 October`.
#[must_use]
pub fn long_day(day: Date) -> String {
    format!("{} {} {}", weekday(day), day.day(), month(day))
}

/// A date with the year: `1 Oct 2026`.
#[must_use]
pub fn short_date(day: Date) -> String {
    format!("{} {} {}", day.day(), &month(day)[..3], day.year())
}

/// A day as a chart or a tooltip writes it: `Sat 10 Oct`.
#[must_use]
pub fn day_label(day: Date) -> String {
    format!("{} {} {}", &weekday(day)[..3], day.day(), &month(day)[..3])
}

/// A moment as its day of the month and time: `10 Oct 13:42`.
#[must_use]
pub fn day_time(at: &Zoned) -> String {
    format!("{} {} {}", at.day(), &month(at.date())[..3], clock(at))
}

/// A moment as a day and time: `Today 11:30`, or `Fri 9 Oct 16:05` on another day.
#[must_use]
pub fn day_and_time(at: &Zoned, now: &Zoned) -> String {
    if at.date() == now.date() {
        format!("Today {}", clock(at))
    } else {
        format!("{} {}", day_label(at.date()), clock(at))
    }
}

/// When a plan window resets: `Resets 14:20, in 2 h 05 min` within a day, and
/// `Resets Thu 15 Oct, 09:00` after that.
#[must_use]
pub fn resets(at: &Zoned, now: &Zoned) -> String {
    let when = if at.date() == now.date() {
        clock(at)
    } else {
        format!("{}, {}", day_label(at.date()), clock(at))
    };
    let left = at.timestamp().as_millisecond() - now.timestamp().as_millisecond();
    if (1..DAY_MS).contains(&left) {
        format!("Resets {when}, in {}", duration(left))
    } else {
        format!("Resets {when}")
    }
}

/// A day's length in milliseconds.
const DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// A month and its year: `October 2026`.
#[must_use]
pub fn month_year(day: Date) -> String {
    format!("{} {}", month(day), day.year())
}

/// A day of the week's name: `Monday`.
#[must_use]
pub fn weekday_name(weekday: Weekday) -> &'static str {
    match weekday {
        Weekday::Monday => "Monday",
        Weekday::Tuesday => "Tuesday",
        Weekday::Wednesday => "Wednesday",
        Weekday::Thursday => "Thursday",
        Weekday::Friday => "Friday",
        Weekday::Saturday => "Saturday",
        Weekday::Sunday => "Sunday",
    }
}

/// A moment's time of day: `14:20`.
#[must_use]
pub fn clock(at: &Zoned) -> String {
    format!("{:02}:{:02}", at.hour(), at.minute())
}

fn weekday(day: Date) -> &'static str {
    weekday_name(day.weekday())
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
#[must_use]
pub fn when(at: &Zoned, now: &Zoned) -> String {
    let time = clock(at);
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
#[must_use]
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
    fn prices_are_to_the_cent_or_a_tenth_of_one() {
        for pair in pairs(&examples(), "price") {
            assert_eq!(price(pair[0].as_f64().unwrap()), pair[1], "{pair}");
        }
    }

    #[test]
    fn shares_are_whole_percentages() {
        for pair in pairs(&examples(), "percent") {
            assert_eq!(percent(pair[0].as_f64().unwrap()), pair[1], "{pair}");
        }
    }

    #[test]
    fn durations_read_as_in_the_design() {
        for pair in pairs(&examples(), "duration") {
            assert_eq!(duration(pair[0].as_i64().unwrap()), pair[1], "{pair}");
        }
    }

    /// The moments of a section of the examples, in its time zone, with its now.
    fn moments(section: &Value) -> (impl Fn(&Value) -> Zoned + '_, Zoned) {
        let zone = jiff::tz::TimeZone::get(section["timeZone"].as_str().unwrap()).unwrap();
        let at = move |ms: &Value| {
            Timestamp::from_millisecond(ms.as_i64().unwrap())
                .unwrap()
                .to_zoned(zone.clone())
        };
        let now = at(&section["nowMs"]);
        (at, now)
    }

    #[test]
    fn days_and_times_read_as_in_the_design() {
        let examples = examples();
        let days = &examples["days"];
        for example in days["examples"].as_array().unwrap() {
            let day: Date = example["day"].as_str().unwrap().parse().unwrap();
            assert_eq!(long_day(day), example["long"], "{example}");
            assert_eq!(short_date(day), example["short"], "{example}");
        }
        for pair in pairs(&examples, "dayLabel") {
            let day: Date = pair[0].as_str().unwrap().parse().unwrap();
            assert_eq!(day_label(day), pair[1], "{pair}");
        }
        for pair in pairs(&examples, "monthYear") {
            let day: Date = pair[0].as_str().unwrap().parse().unwrap();
            assert_eq!(month_year(day), pair[1], "{pair}");
        }
        for (section, format) in [
            ("when", when as fn(&Zoned, &Zoned) -> String),
            ("resets", resets),
            ("dayAndTime", day_and_time),
        ] {
            let (at, now) = moments(&examples[section]);
            for pair in examples[section]["examples"].as_array().unwrap() {
                assert_eq!(format(&at(&pair[0]), &now), pair[1], "{section}: {pair}");
            }
        }
    }

    #[test]
    fn a_moment_reads_as_its_day_and_time() {
        let at: Zoned = "2026-10-10T13:42:00+13:00[Pacific/Auckland]"
            .parse()
            .unwrap();
        assert_eq!(day_time(&at), "10 Oct 13:42");
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
