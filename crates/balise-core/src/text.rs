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

/// The known name closest to a typo, if one is close enough: `quantiy` gives `quantity`,
/// `prot` gives `port`, `production` gives `prod`. Case is ignored.
pub fn closest<'a>(name: &str, known: &[&'a str]) -> Option<&'a str> {
    let name = name.to_ascii_lowercase();
    known
        .iter()
        .filter_map(|candidate| {
            let lower = candidate.to_ascii_lowercase();
            let edits = distance(&name, &lower);
            let close = edits <= (lower.len() / 3).max(1);
            // A shortened or lengthened name: `prod` for `production`.
            let (short, long) = match name.len() <= lower.len() {
                true => (name.as_str(), lower.as_str()),
                false => (lower.as_str(), name.as_str()),
            };
            let prefix = short.len() >= 3 && long.starts_with(short);
            (close || prefix).then_some((edits, *candidate))
        })
        .min_by_key(|(edits, _)| *edits)
        .map(|(_, candidate)| candidate)
}

/// The number of single-character edits from `a` to `b`, a swap of two neighbours counting
/// as one.
fn distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut d = vec![vec![0; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            d[i][j] = (d[i - 1][j] + 1)
                .min(d[i][j - 1] + 1)
                .min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[a.len()][b.len()]
}

/// A size as written in configuration: `2MB`, `512 KB`, `1GB`, `300B`, or a number of bytes.
pub fn parse_size(text: &str) -> Option<usize> {
    let text = text.trim();
    let digits = text
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(text.len());
    let (number, unit) = text.split_at(digits);
    let number: usize = number.parse().ok()?;
    let factor = match unit.trim().to_ascii_uppercase().as_str() {
        "" | "B" => 1,
        "KB" => 1024,
        "MB" => 1024 * 1024,
        "GB" => 1024 * 1024 * 1024,
        _ => return None,
    };
    number.checked_mul(factor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_closest_name_tolerates_swaps_case_and_shortening() {
        let known = ["port", "prod", "quantity", "BALISE_LOG_LEVEL"];
        assert_eq!(closest("prot", &known), Some("port"));
        assert_eq!(closest("production", &known), Some("prod"));
        assert_eq!(closest("quantiy", &known), Some("quantity"));
        assert_eq!(closest("BALISE_LOG_LEVL", &known), Some("BALISE_LOG_LEVEL"));
        assert_eq!(closest("zzz", &known), None);
    }

    #[test]
    fn sizes_are_read_with_their_unit() {
        assert_eq!(parse_size("20MB"), Some(20 * 1024 * 1024));
        assert_eq!(parse_size("512 kb"), Some(512 * 1024));
        assert_eq!(parse_size("300"), Some(300));
        assert_eq!(parse_size("2 TB"), None);
        assert_eq!(parse_size("MB"), None);
    }

    #[test]
    fn sizes_use_the_largest_whole_unit() {
        assert_eq!(format_size(300), "300 bytes");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(2 * 1024 * 1024), "2 MB");
        assert_eq!(format_size(5 * 1024 * 1024 * 1024), "5 GB");
    }
}
