//! Safe conversion of remote names into local path components.
//!
//! Remote metadata is untrusted. Every component derived from a server name
//! goes through [`sanitize_component`] before it is joined onto an output
//! directory, so a name can never escape that directory or produce a path a
//! filesystem rejects.

/// Longest component this module emits, in characters. Leaves room for a
/// collision suffix and for the output directory itself under common path
/// limits.
const MAX_COMPONENT_CHARS: usize = 200;

const FALLBACK: &str = "download";

/// Turn one remote name into a single safe path component.
///
/// Illegal characters, control characters, and path separators are replaced
/// with `_`. A component that is empty, `.`, or `..` becomes `fallback`, which
/// is trusted and used verbatim. Windows device names get a leading `_`, and
/// trailing dots or spaces are stripped because Windows resolves those to the
/// file without them.
pub fn sanitize_component(name: &str, fallback: &str) -> String {
    let fallback = if fallback.trim().is_empty() {
        FALLBACK
    } else {
        fallback
    };

    let mut cleaned = String::with_capacity(name.len());
    for ch in name.chars() {
        let invalid =
            ch.is_control() || matches!(ch, '/' | '\\' | '<' | '>' | ':' | '"' | '|' | '?' | '*');
        if invalid {
            cleaned.push('_');
        } else {
            cleaned.push(ch);
        }
    }

    let cleaned = cleaned.trim_end_matches(['.', ' ']).trim();
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        return fallback.to_string();
    }

    let cleaned = avoid_device_name(cleaned);
    truncate(&cleaned)
}

/// Append ` (n)` before the extension until `candidate` is free.
///
/// `is_taken` answers whether a path is already used, either on disk or by
/// another entry in the same plan.
pub fn unique_name(
    path: &std::path::Path,
    is_taken: &impl Fn(&std::path::Path) -> bool,
) -> std::path::PathBuf {
    if !is_taken(path) {
        return path.to_path_buf();
    }

    let parent = path.parent().unwrap_or_else(|| std::path::Path::new(""));
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "download".to_string());
    let extension = path
        .extension()
        .map(|extension| extension.to_string_lossy().into_owned());

    for counter in 1..=u32::MAX {
        let name = match &extension {
            Some(extension) => format!("{stem} ({counter}).{extension}"),
            None => format!("{stem} ({counter})"),
        };
        let candidate = parent.join(name);
        if !is_taken(&candidate) {
            return candidate;
        }
    }

    unreachable!("u32 counter cannot exhaust before a free name is found")
}

fn avoid_device_name(name: &str) -> String {
    let stem = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || is_numbered_device(&stem, "COM")
        || is_numbered_device(&stem, "LPT");

    if reserved {
        format!("_{name}")
    } else {
        name.to_string()
    }
}

fn is_numbered_device(stem: &str, prefix: &str) -> bool {
    stem.strip_prefix(prefix)
        .is_some_and(|digits| matches!(digits, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9"))
}

fn truncate(name: &str) -> String {
    if name.chars().count() <= MAX_COMPONENT_CHARS {
        return name.to_string();
    }

    let extension = name
        .rfind('.')
        .filter(|index| *index > 0)
        .map(|index| &name[index..])
        .unwrap_or("");
    let extension_chars = extension.chars().count();
    let budget = MAX_COMPONENT_CHARS.saturating_sub(extension_chars);
    let stem_end = name.len().saturating_sub(extension.len());
    let stem: String = name[..stem_end].chars().take(budget).collect();

    if stem.is_empty() {
        return FALLBACK.to_string();
    }
    format!("{stem}{extension}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separators_and_illegal_characters_are_replaced() {
        assert_eq!(sanitize_component("a/b\\c.txt", "fallback"), "a_b_c.txt");
        assert_eq!(
            sanitize_component("bad:name?.txt", "fallback"),
            "bad_name_.txt"
        );
    }

    #[test]
    fn traversal_names_cannot_escape_the_output_directory() {
        assert_eq!(
            sanitize_component("../etc/passwd", "fallback"),
            ".._etc_passwd"
        );
        assert_eq!(sanitize_component("..", "fallback"), "fallback");
        assert_eq!(sanitize_component(".", "fallback"), "fallback");
        assert_eq!(sanitize_component("/", "fallback"), "_");
    }

    #[test]
    fn empty_and_blank_fall_back() {
        assert_eq!(sanitize_component("", "fallback"), "fallback");
        assert_eq!(sanitize_component("   ", "fallback"), "fallback");
        assert_eq!(sanitize_component("", ""), FALLBACK);
    }

    #[test]
    fn windows_device_names_are_prefixed() {
        assert_eq!(sanitize_component("CON", "fallback"), "_CON");
        assert_eq!(sanitize_component("nul.txt", "fallback"), "_nul.txt");
        assert_eq!(sanitize_component("LPT1", "fallback"), "_LPT1");
        assert_eq!(sanitize_component("COM10", "fallback"), "COM10");
        assert_eq!(sanitize_component("console", "fallback"), "console");
    }

    #[test]
    fn trailing_dots_and_spaces_are_stripped() {
        assert_eq!(sanitize_component("name. ", "fallback"), "name");
        assert_eq!(sanitize_component("name...", "fallback"), "name");
    }

    #[test]
    fn long_names_keep_their_extension() {
        let name = format!("{}.mp4", "a".repeat(300));
        let sanitized = sanitize_component(&name, "fallback");
        assert_eq!(sanitized.chars().count(), MAX_COMPONENT_CHARS);
        assert!(sanitized.ends_with(".mp4"));
    }

    #[test]
    fn unique_name_picks_the_first_free_suffix() {
        let taken = ["clip.mp4", "clip (1).mp4"].map(std::path::PathBuf::from);
        let path = std::path::Path::new("clip.mp4");
        let result = unique_name(path, &|candidate| taken.contains(&candidate.to_path_buf()));
        assert_eq!(result, std::path::PathBuf::from("clip (2).mp4"));
    }
}
