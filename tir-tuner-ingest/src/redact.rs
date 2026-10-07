//! Strips the patient name from a Glooko banner.

/// Replaces the value after `Name:` in the first line; the rest stays byte-for-byte.
pub(crate) fn redact(text: &str) -> String {
    let line_end = text.find('\n').unwrap_or(text.len());
    let Some(start) = text[..line_end].find("Name:").map(|i| i + "Name:".len()) else {
        return text.to_string();
    };
    let end = text[start..line_end]
        .find(',')
        .map_or(line_end, |i| start + i);
    format!("{}REDACTED{}", &text[..start], &text[end..])
}

#[cfg(test)]
mod tests {
    #[test]
    fn replaces_only_the_name() {
        let raw = "Name:Erika Mustermann,Date Range:2026-09-24 - 2026-10-07\nTimestamp,Name\n2026-10-07 02:32,x\n";
        assert_eq!(
            super::redact(raw),
            "Name:REDACTED,Date Range:2026-09-24 - 2026-10-07\nTimestamp,Name\n2026-10-07 02:32,x\n"
        );
    }
}
