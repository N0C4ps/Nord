//! Logging facade for Nord Tech.

/// Emit an informational bootstrap message.
pub fn info(message: &str) {
    println!("[INFO] {message}");
}
