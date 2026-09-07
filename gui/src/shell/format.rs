pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

pub fn relative_time(opened_at: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(opened_at);
    let delta = now.saturating_sub(opened_at);
    if delta < 60 {
        return "just now".to_string();
    }
    if delta < 3600 {
        let mins = delta / 60;
        return format!("{mins} min ago");
    }
    if delta < 86_400 {
        let hours = delta / 3600;
        return format!("{hours} hr ago");
    }
    let days = delta / 86_400;
    format!("{days} d ago")
}
