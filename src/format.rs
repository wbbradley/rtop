use std::time::Duration;

const KIB: u64 = 1 << 10;
const MIB: u64 = 1 << 20;
const GIB: u64 = 1 << 30;

const SECS_PER_MIN: u64 = 60;
const SECS_PER_HOUR: u64 = 60 * 60;
const SECS_PER_DAY: u64 = 24 * 60 * 60;

pub fn bytes(b: u64) -> String {
    if b < KIB {
        return format!("{b}B");
    }
    let (div, unit) = if b >= GIB {
        (GIB, "GiB")
    } else if b >= MIB {
        (MIB, "MiB")
    } else {
        (KIB, "KiB")
    };
    let value = b as f64 / div as f64;
    if value < 10.0 {
        format!("{value:.1}{unit}")
    } else {
        format!("{value:.0}{unit}")
    }
}

pub fn age(d: Duration) -> String {
    let total = d.as_secs();
    if total >= SECS_PER_DAY {
        let days = total / SECS_PER_DAY;
        let hours = (total % SECS_PER_DAY) / SECS_PER_HOUR;
        format!("{days}d{hours}h")
    } else if total >= SECS_PER_HOUR {
        let hours = total / SECS_PER_HOUR;
        let mins = (total % SECS_PER_HOUR) / SECS_PER_MIN;
        format!("{hours}h{mins:02}m")
    } else if total >= SECS_PER_MIN {
        let mins = total / SECS_PER_MIN;
        let secs = total % SECS_PER_MIN;
        format!("{mins}m{secs:02}s")
    } else {
        format!("{total}s")
    }
}

// Phase 7 will refine the boundary set; this is the simplified Phase 2 helper
// reused for both TIME+ and AGE rendering.
pub fn time_plus(d: Duration) -> String {
    let total = d.as_secs();
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{h}h{m:02}m")
    } else if m > 0 {
        format!("{m}m{s:02}s")
    } else {
        format!("{s}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_zero() {
        assert_eq!(bytes(0), "0B");
    }

    #[test]
    fn bytes_below_kib() {
        assert_eq!(bytes(1), "1B");
        assert_eq!(bytes(1023), "1023B");
    }

    #[test]
    fn bytes_kib_boundary() {
        assert_eq!(bytes(1024), "1.0KiB");
        assert_eq!(bytes(12 * KIB), "12KiB");
    }

    #[test]
    fn bytes_mib_boundary() {
        assert_eq!(bytes(MIB - 1), "1024KiB");
        assert_eq!(bytes(MIB), "1.0MiB");
        assert_eq!(bytes(345 * MIB), "345MiB");
    }

    #[test]
    fn bytes_gib_boundary() {
        // 1.2 GiB
        let b = (1.2_f64 * GIB as f64) as u64;
        assert_eq!(bytes(b), "1.2GiB");
    }

    #[test]
    fn bytes_just_under_ten_mib_rounding() {
        // Pin Rust's default rounding behavior at the < 10 / >= 10 boundary.
        // 9.95 MiB → expressed in bytes, then rendered with "{:.1}".
        let b = (9.95_f64 * MIB as f64) as u64;
        let rendered = bytes(b);
        // Must use MiB (not KiB) and a single decimal.
        assert!(rendered.ends_with("MiB"), "got {rendered}");
        assert!(
            rendered == "9.9MiB" || rendered == "10.0MiB",
            "got {rendered}"
        );
    }

    #[test]
    fn bytes_clamp_at_gib() {
        // Above 1024 GiB still rendered as GiB (P1 spec).
        let b: u64 = 2048u64 * GIB;
        assert!(bytes(b).ends_with("GiB"));
    }

    #[test]
    fn age_zero() {
        assert_eq!(age(Duration::from_secs(0)), "0s");
    }

    #[test]
    fn age_seconds_under_minute() {
        assert_eq!(age(Duration::from_secs(32)), "32s");
    }

    #[test]
    fn age_just_under_a_minute() {
        assert_eq!(age(Duration::from_secs(59)), "59s");
    }

    #[test]
    fn age_minute_boundary() {
        assert_eq!(age(Duration::from_secs(60)), "1m00s");
    }

    #[test]
    fn age_minutes_seconds() {
        assert_eq!(age(Duration::from_secs(12 * 60 + 32)), "12m32s");
    }

    #[test]
    fn age_hour_boundary() {
        assert_eq!(age(Duration::from_secs(3600)), "1h00m");
    }

    #[test]
    fn age_hours_minutes() {
        assert_eq!(age(Duration::from_secs(4 * 3600 + 12 * 60 + 5)), "4h12m");
    }

    #[test]
    fn age_just_under_a_day() {
        assert_eq!(age(Duration::from_secs(24 * 3600 - 1)), "23h59m");
    }

    #[test]
    fn age_day_boundary() {
        assert_eq!(age(Duration::from_secs(24 * 3600)), "1d0h");
    }

    #[test]
    fn age_days_hours() {
        assert_eq!(age(Duration::from_secs(86400 + 4 * 3600 + 30 * 60)), "1d4h");
    }

    #[test]
    fn time_plus_zero() {
        assert_eq!(time_plus(Duration::ZERO), "0s");
    }

    #[test]
    fn time_plus_seconds() {
        assert_eq!(time_plus(Duration::from_secs(45)), "45s");
    }

    #[test]
    fn time_plus_minutes() {
        assert_eq!(time_plus(Duration::from_secs(12 * 60 + 45)), "12m45s");
    }

    #[test]
    fn time_plus_hours() {
        assert_eq!(time_plus(Duration::from_secs(3600 + 23 * 60)), "1h23m");
    }
}
