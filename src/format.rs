const KIB: u64 = 1 << 10;
const MIB: u64 = 1 << 20;
const GIB: u64 = 1 << 30;

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
        let b = (9.95_f64 * MIB as f64) as u64;
        let rendered = bytes(b);
        assert!(rendered.ends_with("MiB"), "got {rendered}");
        assert!(
            rendered == "9.9MiB" || rendered == "10.0MiB",
            "got {rendered}"
        );
    }

    #[test]
    fn bytes_clamp_at_gib() {
        let b: u64 = 2048u64 * GIB;
        assert!(bytes(b).ends_with("GiB"));
    }
}
