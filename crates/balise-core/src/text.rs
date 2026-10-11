//! Small text helpers shared by the router, the extractors and messages.

/// Decodes `%XX` sequences; anything that is not valid UTF-8 afterwards is replaced.
pub fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(high), Some(low)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
        {
            decoded.push((high * 16 + low) as u8);
            i += 3;
        } else {
            decoded.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

/// A size as messages show it: `20 MB`, `1.5 KB`, `300 bytes`.
pub fn format_size(bytes: usize) -> String {
    const UNITS: [&str; 3] = ["KB", "MB", "GB"];
    if bytes < 1024 {
        return format!("{bytes} bytes");
    }
    let mut size = bytes as f64 / 1024.0;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    let text = format!("{size:.1}");
    let text = text.strip_suffix(".0").unwrap_or(&text);
    format!("{text} {}", UNITS[unit])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_use_the_largest_whole_unit() {
        assert_eq!(format_size(300), "300 bytes");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(2 * 1024 * 1024), "2 MB");
        assert_eq!(format_size(5 * 1024 * 1024 * 1024), "5 GB");
    }
}
