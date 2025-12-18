//! UI Utilities
//!
//! Progress bars, colored output, and terminal helpers.

use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};

/// Print a styled header
pub fn print_header(text: &str) {
    println!();
    println!("  {} {}", "▶".cyan().bold(), text.bold());
    println!();
}

/// Print a success message
pub fn print_success(text: &str) {
    println!("  {} {}", "✓".green().bold(), text.green());
}

/// Print a warning message
pub fn print_warning(text: &str) {
    println!("  {} {}", "⚠".yellow().bold(), text.yellow());
}

/// Print an error message
pub fn print_error(text: &str) {
    println!("  {} {}", "✗".red().bold(), text.red());
}

/// Create a spinner progress bar
pub fn create_spinner(message: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::with_template("  {spinner:.cyan} {msg}")
            .unwrap()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
    );
    pb.set_message(message.to_string());
    pb.enable_steady_tick(std::time::Duration::from_millis(80));
    pb
}

/// Create a progress bar with known length
pub fn create_progress_bar(len: u64, message: &str) -> ProgressBar {
    let pb = ProgressBar::new(len);
    pb.set_style(
        ProgressStyle::with_template(
            "  {spinner:.cyan} {msg} [{bar:30.cyan/dim}] {pos}/{len}",
        )
        .unwrap()
        .progress_chars("━━╺"),
    );
    pb.set_message(message.to_string());
    pb
}

/// Print a key-value pair
pub fn print_kv(key: &str, value: &str) {
    println!("  {}: {}", key.dimmed(), value);
}

/// Print a bullet point
pub fn print_bullet(text: &str) {
    println!("  {} {}", "•".dimmed(), text);
}
