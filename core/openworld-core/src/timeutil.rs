// SPDX-License-Identifier: Apache-2.0

use std::time::{Duration, SystemTime};

pub fn parse_rfc3339(text: &str) -> Option<SystemTime> {
    let text = text.trim();
    if text.len() < 20 || !text.ends_with('Z') {
        return None;
    }
    let year: i32 = text.get(0..4)?.parse().ok()?;
    let month: u32 = text.get(5..7)?.parse().ok()?;
    let day: u32 = text.get(8..10)?.parse().ok()?;
    if text.as_bytes().get(10) != Some(&b'T') {
        return None;
    }
    let hour: u32 = text.get(11..13)?.parse().ok()?;
    let min: u32 = text.get(14..16)?.parse().ok()?;
    let sec: u32 = text.get(17..19)?.parse().ok()?;
    if hour > 23 || min > 59 || sec > 60 {
        return None;
    }
    let days = days_from_civil(year, month, day)?;
    let secs = days * 86400 + i64::from(hour) * 3600 + i64::from(min) * 60 + i64::from(sec);
    if secs < 0 {
        return None;
    }
    Some(SystemTime::UNIX_EPOCH + Duration::from_secs(secs as u64))
}

pub fn format_rfc3339(time: SystemTime) -> String {
    let secs = time.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_secs() as i64;
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400) as u32;
    let (year, month, day) = civil_from_days(days);
    let hour = rem / 3600;
    let min = (rem % 3600) / 60;
    let sec = rem % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{min:02}:{sec:02}Z")
}

fn days_from_civil(year: i32, month: u32, day: u32) -> Option<i64> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let y = year as i64;
    let m = month as i64;
    let d = day as i64;
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy as u64;
    Some(era * 146097 + doe as i64 - 719468)
}

fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as u32, d as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_epoch_roundtrip() {
        let t = parse_rfc3339("1970-01-01T00:00:00Z").unwrap();
        assert_eq!(t, SystemTime::UNIX_EPOCH);
        assert_eq!(format_rfc3339(t), "1970-01-01T00:00:00Z");
        let later = parse_rfc3339("2026-10-07T05:09:00Z").unwrap();
        assert_eq!(format_rfc3339(later), "2026-10-07T05:09:00Z");
    }
}
