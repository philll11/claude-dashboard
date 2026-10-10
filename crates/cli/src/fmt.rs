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

    #[test]
    fn counts_read_naturally() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(2_775), "2,775");
        assert_eq!(grouped(1_234_567), "1,234,567");
    }

    #[test]
    fn token_counts_are_compact_as_the_design_writes_them() {
        for (count, text) in [
            (640, "640"),
            (3_200, "3K"),
            (166_400, "166K"),
            (999_499, "999K"),
            (999_500, "1.0M"),
            (1_040_000, "1.0M"),
            (12_340_000, "12.3M"),
            (150_000_000, "150M"),
            (11_496_979_979, "11.5B"),
        ] {
            assert_eq!(compact(count), text, "{count}");
        }
    }

    #[test]
    fn money_is_to_the_cent() {
        assert_eq!(money(5.049), "$5.05");
        assert_eq!(money(0.0), "$0.00");
        assert_eq!(money(1_380.574), "$1,380.57");
    }

    #[test]
    fn days_and_times_read_as_in_the_design() {
        let now: Zoned = "2026-10-10T12:15:00+13:00[Pacific/Auckland]"
            .parse()
            .unwrap();
        assert_eq!(long_day(now.date()), "Saturday 10 October");
        assert_eq!(short_date(now.date()), "10 Oct 2026");
        let at = |text: &str| text.parse::<Zoned>().unwrap();
        assert_eq!(
            when(&at("2026-10-10T14:20:00+13:00[Pacific/Auckland]"), &now),
            "14:20"
        );
        assert_eq!(
            when(&at("2026-10-15T09:00:00+13:00[Pacific/Auckland]"), &now),
            "Thu 09:00"
        );
        assert_eq!(
            when(&at("2026-10-20T09:00:00+13:00[Pacific/Auckland]"), &now),
            "20 Oct 09:00"
        );
    }

    #[test]
    fn ages_read_naturally() {
        let now: Timestamp = "2026-10-10T12:00:00Z".parse().unwrap();
        let before = |seconds: i64| Timestamp::from_second(now.as_second() - seconds).unwrap();
        assert_eq!(ago(before(20), now), "just now");
        assert_eq!(ago(before(120), now), "2 min ago");
        assert_eq!(ago(before(3 * 3600 + 600), now), "3 h ago");
        assert_eq!(ago(before(86_400), now), "1 day ago");
        assert_eq!(ago(before(4 * 86_400), now), "4 days ago");
    }
}
