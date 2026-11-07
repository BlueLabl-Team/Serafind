//! Sizes and dates for the `rows = "details"` layout.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// `245 KB`, `1.2 MB`... (powers of 1000, as file managers show them).
#[must_use]
pub fn size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes;
    let mut unit = 0;
    let mut remainder = 0;
    while value >= 1000 && unit < UNITS.len() - 1 {
        remainder = value % 1000;
        value /= 1000;
        unit += 1;
    }
    if unit == 0 || value >= 10 {
        format!("{value} {}", UNITS[unit])
    } else {
        format!("{value}.{} {}", remainder / 100, UNITS[unit])
    }
}

/// `2026-03-12` (UTC).
#[must_use]
pub fn date(time: SystemTime) -> String {
    let days = time
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs() / 86_400);
    let (year, month, day) = civil_from_days(i64::try_from(days).unwrap_or(0));
    format!("{year:04}-{month:02}-{day:02}")
}

/// Size (files only) and modification date of a path, read from the disk.
#[must_use]
pub fn details(path: &Path) -> (Option<String>, Option<String>) {
    let Ok(metadata) = std::fs::metadata(path) else {
        return (None, None);
    };
    let size = metadata.is_file().then(|| size(metadata.len()));
    let date = metadata.modified().ok().map(date);
    (size, date)
}

/// Days since 1970-01-01 to (year, month, day), from Howard Hinnant's algorithm.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = u32::try_from(day_of_year - (153 * month_index + 2) / 5 + 1).unwrap_or(1);
    let month = u32::try_from(if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    })
    .unwrap_or(1);
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use std::time::Duration;

    #[rstest]
    #[case(0, "0 B")]
    #[case(999, "999 B")]
    #[case(1000, "1.0 KB")]
    #[case(1_234, "1.2 KB")]
    #[case(245_000, "245 KB")]
    #[case(12_500_000, "12 MB")]
    #[case(3_100_000_000, "3.1 GB")]
    fn sizes(#[case] bytes: u64, #[case] text: &str) {
        assert_eq!(size(bytes), text);
    }

    #[rstest]
    #[case(0, "1970-01-01")]
    #[case(951_782_400, "2000-02-29")]
    #[case(1_794_000_000, "2026-11-06")]
    fn dates(#[case] seconds: u64, #[case] text: &str) {
        assert_eq!(date(UNIX_EPOCH + Duration::from_secs(seconds)), text);
    }
}
