use std::fmt;
use std::num::ParseFloatError;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct FileSize {
    bytes: u64,
}

impl FileSize {
    pub const MAX: Self = Self { bytes: u64::MAX };

    /// Whether this is the [`MAX`](Self::MAX) sentinel, used by uploaders
    /// without a size cap.
    pub const fn is_unbounded(&self) -> bool {
        self.bytes == u64::MAX
    }

    pub const fn from_bytes(bytes: u64) -> Self {
        Self { bytes }
    }

    pub const fn from_kb(kb: u64) -> Self {
        Self { bytes: kb * 1000 }
    }

    pub const fn from_mb(mb: u64) -> Self {
        Self {
            bytes: mb * 1_000_000,
        }
    }

    pub const fn from_gb(gb: u64) -> Self {
        Self {
            bytes: gb * 1_000_000_000,
        }
    }

    pub const fn from_tb(tb: u64) -> Self {
        Self {
            bytes: tb * 1_000_000_000_000,
        }
    }

    pub const fn from_kib(kib: u64) -> Self {
        Self { bytes: kib * 1024 }
    }

    pub const fn from_mib(mib: u64) -> Self {
        Self {
            bytes: mib * 1024 * 1024,
        }
    }

    pub const fn from_gib(gib: u64) -> Self {
        Self {
            bytes: gib * 1024 * 1024 * 1024,
        }
    }

    pub const fn as_bytes(&self) -> u64 {
        self.bytes
    }

    pub const fn as_kb(&self) -> u64 {
        self.bytes / 1000
    }

    pub const fn as_mb(&self) -> u64 {
        self.bytes / 1_000_000
    }

    pub const fn as_gb(&self) -> u64 {
        self.bytes / 1_000_000_000
    }

    pub const fn as_tb(&self) -> u64 {
        self.bytes / 1_000_000_000_000
    }

    pub fn as_f64_bytes(&self) -> f64 {
        self.bytes as f64
    }

    pub fn as_f64_kb(&self) -> f64 {
        self.bytes as f64 / 1000.0
    }

    pub fn as_f64_mb(&self) -> f64 {
        self.bytes as f64 / 1_000_000.0
    }

    pub fn as_f64_gb(&self) -> f64 {
        self.bytes as f64 / 1_000_000_000.0
    }

    pub fn as_f64_tb(&self) -> f64 {
        self.bytes as f64 / 1_000_000_000_000.0
    }

    pub fn to_human_string(&self) -> String {
        const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
        let bytes = self.bytes as f64;
        if bytes < 1000.0 {
            return format!("{:.0} B", bytes);
        }
        let exp = (bytes.log10() / 1000.0_f64.log10()).floor() as usize;
        let exp = exp.min(UNITS.len() - 1);
        let value = bytes / 1000.0_f64.powi(exp as i32);
        format!("{:.1} {}", value, UNITS[exp])
    }
}

impl FromStr for FileSize {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        if s.is_empty() {
            return Err("empty string".to_string());
        }

        let mut num_end = s.len();
        for (i, ch) in s.chars().enumerate() {
            if ch.is_alphabetic() || ch == ' ' {
                num_end = i;
                break;
            }
        }
        if num_end == 0 {
            return Err("no numeric part found".to_string());
        }
        let num_str = &s[..num_end];
        let unit_str = &s[num_end..].trim();

        let value: f64 = num_str
            .parse()
            .map_err(|e: ParseFloatError| e.to_string())?;

        let unit = unit_str.to_uppercase();
        let unit = unit.trim_end_matches('B').trim();

        let bytes = match unit {
            "" | "B" => value,
            "K" | "KB" => value * 1000.0,
            "M" | "MB" => value * 1_000_000.0,
            "G" | "GB" => value * 1_000_000_000.0,
            "T" | "TB" => value * 1_000_000_000_000.0,
            "KI" | "KIB" => value * 1024.0,
            "MI" | "MIB" => value * 1024.0 * 1024.0,
            "GI" | "GIB" => value * 1024.0 * 1024.0 * 1024.0,
            _ => return Err(format!("unknown unit '{}'", unit_str)),
        };

        if bytes < 0.0 {
            return Err("negative size".to_string());
        }
        if bytes > u64::MAX as f64 {
            return Err(format!("size {} exceeds u64 maximum", bytes));
        }
        Ok(FileSize::from_bytes(bytes as u64))
    }
}

impl fmt::Display for FileSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_human_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructors_and_getters() {
        let b = FileSize::from_bytes(1_500_000);
        assert_eq!(b.as_bytes(), 1_500_000);
        assert_eq!(b.as_kb(), 1500);
        assert_eq!(b.as_mb(), 1);
        assert_eq!(b.as_gb(), 0);
        assert_eq!(b.as_tb(), 0);

        assert_eq!(FileSize::from_kb(2).as_bytes(), 2000);
        assert_eq!(FileSize::from_mb(3).as_bytes(), 3_000_000);
        assert_eq!(FileSize::from_gb(4).as_bytes(), 4_000_000_000);
        assert_eq!(FileSize::from_tb(1).as_bytes(), 1_000_000_000_000);
        assert_eq!(FileSize::from_kib(1).as_bytes(), 1024);
        assert_eq!(FileSize::from_mib(1).as_bytes(), 1_048_576);
        assert_eq!(FileSize::from_gib(1).as_bytes(), 1_073_741_824);
    }

    #[test]
    fn unbounded_sentinel() {
        assert!(FileSize::MAX.is_unbounded());
        assert!(!FileSize::from_bytes(0).is_unbounded());
        assert!(!FileSize::from_tb(1).is_unbounded());
    }

    #[test]
    fn float_getters() {
        let size = FileSize::from_bytes(2_500_000);
        assert_eq!(size.as_f64_bytes(), 2_500_000.0);
        assert_eq!(size.as_f64_kb(), 2500.0);
        assert_eq!(size.as_f64_mb(), 2.5);
        assert_eq!(size.as_f64_gb(), 0.0025);
        assert_eq!(size.as_f64_tb(), 0.0000025);
    }

    #[test]
    fn comparison() {
        let a = FileSize::from_mb(1);
        let b = FileSize::from_kb(1000);
        assert_eq!(a, b);
        assert!(a <= b);
        let c = FileSize::from_bytes(1_500_000);
        assert!(c > a);
    }

    #[test]
    fn human_string() {
        assert_eq!(FileSize::from_bytes(500).to_human_string(), "500 B");
        assert_eq!(FileSize::from_bytes(1500).to_human_string(), "1.5 KB");
        assert_eq!(FileSize::from_mb(1).to_human_string(), "1.0 MB");
        assert_eq!(FileSize::from_mb(1234).to_human_string(), "1.2 GB");
        assert_eq!(FileSize::from_tb(1).to_human_string(), "1.0 TB");
    }

    #[test]
    fn display_impl() {
        assert_eq!(format!("{}", FileSize::from_mb(2)), "2.0 MB");
    }

    #[test]
    fn parse_decimal() {
        let cases = vec![
            ("1B", 1),
            ("2 KB", 2000),
            ("3.5MB", 3_500_000),
            ("0.001GB", 1_000_000),
            ("4 TB", 4_000_000_000_000),
        ];
        for (s, expected) in cases {
            let parsed: FileSize = s.parse().unwrap();
            assert_eq!(parsed.as_bytes(), expected);
        }
    }

    #[test]
    fn parse_binary() {
        let cases = vec![
            ("1KiB", 1024),
            ("2 MiB", 2_097_152),
            ("1.5GiB", 1_610_612_736),
        ];
        for (s, expected) in cases {
            let parsed: FileSize = s.parse().unwrap();
            assert_eq!(parsed.as_bytes(), expected);
        }
    }

    #[test]
    fn parse_case_insensitive() {
        let s = "5mb";
        let parsed: FileSize = s.parse().unwrap();
        assert_eq!(parsed.as_bytes(), 5_000_000);
    }

    #[test]
    fn parse_errors() {
        assert!("".parse::<FileSize>().is_err());
        assert!("abc".parse::<FileSize>().is_err());
        assert!("-1KB".parse::<FileSize>().is_err());
        assert!("10XYZ".parse::<FileSize>().is_err());
    }

    #[test]
    fn max_size() {
        let max = FileSize::from_gb(2);
        let small = FileSize::from_mb(1500);
        let large = FileSize::from_mb(2500);
        assert!(small <= max);
        assert!(large > max);
    }
}
