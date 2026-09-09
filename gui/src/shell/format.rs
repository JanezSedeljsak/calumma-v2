use super::Catalog;

pub fn format_bytes(bytes: u64, l10n: &Catalog) -> String {
    const KEYS: [&str; 5] = [
        "byteUnit",
        "kilobyteUnit",
        "megabyteUnit",
        "gigabyteUnit",
        "terabyteUnit",
    ];
    if bytes < 1024 {
        return l10n.format("byteUnit", &[&bytes.to_string()]);
    }
    let mut value = bytes as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit < KEYS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    l10n.format(KEYS[unit], &[&format!("{value:.1}")])
}

pub fn relative_time(opened_at: i64, l10n: &Catalog) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(opened_at);
    let delta = now.saturating_sub(opened_at);
    if delta < 60 {
        return l10n.get("justNow");
    }
    if delta < 3600 {
        return l10n.format("minutesAgo", &[&(delta / 60).to_string()]);
    }
    if delta < 86_400 {
        return l10n.format("hoursAgo", &[&(delta / 3600).to_string()]);
    }
    l10n.format("daysAgo", &[&(delta / 86_400).to_string()])
}
