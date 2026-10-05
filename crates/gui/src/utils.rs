//! Small platform helpers shared by views.

use upio::filesize::FileSize;
use upio::registry::UploaderId;

/// The display name for a service (config keys stay lowercase).
pub fn service_display_name(id: UploaderId) -> &'static str {
    match id {
        UploaderId::Bunkr => "Bunkr",
        UploaderId::Gofile => "GoFile",
        UploaderId::Fileditch => "Fileditch",
        UploaderId::Filester => "Filester",
        UploaderId::Goonbox => "GoonBox",
    }
}

/// Copy text to the system clipboard.
///
/// Uses the OS clipboard directly (`arboard`) — the webview's
/// `navigator.clipboard` is unreliable in desktop windows.
pub fn copy_text(text: &str) {
    match arboard::Clipboard::new().and_then(|mut clipboard| clipboard.set_text(text)) {
        Ok(()) => {}
        Err(e) => {
            dioxus::logger::tracing::error!("failed to copy to clipboard: {e}");
        }
    }
}

/// Reveal a file in the OS file manager (or open it with the default app).
pub fn reveal_path(path: &std::path::Path) {
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("explorer").arg(path).spawn();

    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(path).spawn();

    #[cfg(all(unix, not(target_os = "macos")))]
    let result = std::process::Command::new("xdg-open").arg(path).spawn();

    if let Err(e) = result {
        dioxus::logger::tracing::warn!("failed to reveal path: {e}");
    }
}

/// Format a size for display.
///
/// The unbounded sentinel (`FileSize::MAX`, used by services without a size
/// cap) renders as "No limit" instead of an absurd unit; everything else
/// delegates to [`FileSize::to_human_string`].
pub fn format_size(size: FileSize) -> String {
    if size.is_unbounded() {
        "No limit".to_string()
    } else {
        size.to_human_string()
    }
}
