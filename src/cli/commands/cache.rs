//! `axur cache` — inspect or clear the content cache.
//!
//! Everything stored is immutable content addressed by commit, so clearing is
//! always safe: the next sync refetches and repopulates.

use anyhow::Result;

use crate::core::cache::Cache;
use crate::utils::ui;

fn human(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[0])
    } else {
        format!("{:.1} {}", value, UNITS[unit])
    }
}

pub async fn execute(clear: bool) -> Result<()> {
    ui::intro("axur cache");

    let cache = Cache::open();

    if !cache.is_enabled() {
        ui::warning("Caching is disabled by AXUR_NO_CACHE");
        ui::outro("Nothing to report");
        return Ok(());
    }

    if clear {
        let removed = cache.clear()?;
        ui::success(&format!("Cleared {} entries", removed));
        ui::outro("The next sync will refetch and repopulate");
        return Ok(());
    }

    let (bytes, count) = cache.stats();
    if count == 0 {
        ui::step("Empty — the next sync will populate it");
    } else {
        ui::step(&format!(
            "{} entries  {}  {}\n{}",
            count,
            ui::dim("·"),
            human(bytes),
            ui::dim("Content is pinned to commits, so entries never go stale.")
        ));
    }

    ui::outro(&format!("Clear with {}", ui::accent("axur cache --clear")));

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::human;

    #[test]
    fn formats_sizes_readably() {
        assert_eq!(human(0), "0 B");
        assert_eq!(human(512), "512 B");
        assert_eq!(human(2048), "2.0 KB");
        assert_eq!(human(5 * 1024 * 1024), "5.0 MB");
    }
}
