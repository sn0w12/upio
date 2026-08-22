use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use walkdir::WalkDir;

/// Collect files from explicit paths, directories, and glob patterns.
///
/// Directories are walked recursively, skipping hidden entries. Results are
/// de-duplicated and sorted deterministically.
pub fn collect_files(paths: &[String], globs: &[String]) -> Result<Vec<String>> {
    let mut files = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for path in paths {
        let path = Path::new(path);
        if path.is_file() {
            push_file(path.to_path_buf(), &mut files, &mut seen);
        } else if path.is_dir() {
            walk(path, &mut |p| push_file(p, &mut files, &mut seen))?;
        } else {
            bail!("Invalid path: {}", path.display());
        }
    }

    for pattern in globs {
        collect_glob(pattern, &mut |p| push_file(p, &mut files, &mut seen))?;
    }

    files.sort();
    Ok(files)
}

fn push_file(path: PathBuf, files: &mut Vec<String>, seen: &mut std::collections::HashSet<String>) {
    let key = path.to_string_lossy().into_owned();
    if seen.insert(key.clone()) {
        files.push(key);
    }
}

fn walk<F: FnMut(PathBuf)>(root: &Path, push: &mut F) -> Result<()> {
    for entry in WalkDir::new(root).into_iter().filter_entry(should_descend) {
        let entry = entry.map_err(|e| {
            anyhow::anyhow!(
                "cannot read '{}': {}",
                e.path()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default(),
                e
            )
        })?;
        if entry.file_type().is_file() && !is_hidden(entry.path()) {
            push(entry.into_path());
        }
    }
    Ok(())
}

fn collect_glob<F: FnMut(PathBuf)>(pattern: &str, push: &mut F) -> Result<()> {
    let has_glob = pattern.contains('*') || pattern.contains('?') || pattern.contains('[');
    if !has_glob {
        let path = PathBuf::from(pattern);
        if path.is_file() {
            push(path);
        }
        return Ok(());
    }

    let base = glob_base(pattern);
    let pattern = normalize(pattern);
    let base_norm = normalize(&base.to_string_lossy())
        .trim_end_matches('/')
        .to_string();
    // Strip the base prefix from the pattern so both sides are relative.
    let pattern = pattern
        .strip_prefix(&base_norm)
        .unwrap_or(&pattern)
        .trim_start_matches('/')
        .to_string();
    for entry in WalkDir::new(&base).into_iter().filter_entry(should_descend) {
        let entry = entry.map_err(|e| {
            anyhow::anyhow!(
                "cannot read '{}': {}",
                e.path()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default(),
                e
            )
        })?;
        if !entry.file_type().is_file() || is_hidden(entry.path()) {
            continue;
        }
        let path = entry.path();
        let full = normalize(&path.to_string_lossy());
        let relative = full.strip_prefix(&base_norm).unwrap_or(&full);
        let relative = relative.trim_start_matches('/');
        if glob_match(&pattern, relative) {
            push(path.to_path_buf());
        }
    }
    Ok(())
}

fn should_descend(entry: &walkdir::DirEntry) -> bool {
    entry.depth() == 0 || !is_hidden(entry.path())
}

fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .map(|name| name.to_string_lossy().starts_with('.'))
        .unwrap_or(false)
}

/// The deepest non-glob ancestor of a pattern, used as the walk root.
fn glob_base(pattern: &str) -> PathBuf {
    // Split the pattern at the first glob metacharacter and keep the literal
    // prefix. Works with `Path::ancestors` semantics by treating the prefix as
    // a plain path.
    let glob_idx = pattern.find(['*', '?', '[']);
    match glob_idx {
        None => {
            let path = PathBuf::from(pattern);
            if path.is_dir() {
                path
            } else {
                PathBuf::from(".")
            }
        }
        Some(idx) => {
            let prefix = &pattern[..idx];
            let trimmed = prefix.trim_end_matches(['/', '\\']);
            if trimmed.is_empty() {
                PathBuf::from(".")
            } else {
                PathBuf::from(trimmed)
            }
        }
    }
}

fn normalize(s: &str) -> String {
    s.replace('\\', "/")
}

fn glob_match(pattern: &str, path: &str) -> bool {
    let pattern: Vec<&str> = pattern.split('/').filter(|s| !s.is_empty()).collect();
    let path: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    match_segments(&pattern, &path)
}

/// Two-segment wildcard matcher (`*` and `**` across components) with
/// memoization over (pattern index, path index) so pathological inputs stay
/// polynomial.
fn match_segments(pattern: &[&str], path: &[&str]) -> bool {
    let mut memo = std::collections::HashSet::new();
    go_segments(pattern, path, 0, 0, &mut memo)
}

fn go_segments(
    pattern: &[&str],
    path: &[&str],
    pi: usize,
    ti: usize,
    seen: &mut std::collections::HashSet<(usize, usize)>,
) -> bool {
    if pi == pattern.len() {
        return ti == path.len();
    }
    if ti == path.len() {
        // A trailing run of segments where every remaining pattern segment
        // is `*`-only can still match zero path segments (each star matches
        // an empty component).
        return pattern[pi..]
            .iter()
            .all(|s| !s.is_empty() && s.chars().all(|c| c == '*'));
    }
    if pattern[pi] != "**" {
        return component_match(pattern[pi], path[ti])
            && go_segments(pattern, path, pi + 1, ti + 1, seen);
    }
    // `**`: either consume one path segment or match zero.
    if !seen.insert((pi, ti)) {
        return false;
    }
    go_segments(pattern, path, pi, ti + 1, seen) || go_segments(pattern, path, pi + 1, ti, seen)
}

fn component_match(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let mut memo = std::collections::HashSet::new();
    match_component(&pattern, &text, 0, 0, &mut memo)
}

/// Single-component wildcard matching with `*`, `?`, and `[...]` classes
/// (single chars, ranges `a-z`, negation via leading `!` or `^`; a malformed
/// class matches no character).
fn match_component(
    pattern: &[char],
    text: &[char],
    pi: usize,
    ti: usize,
    seen: &mut std::collections::HashSet<(usize, usize)>,
) -> bool {
    if pi == pattern.len() {
        return ti == text.len();
    }
    if !seen.insert((pi, ti)) {
        return false;
    }
    match pattern[pi] {
        '*' => {
            // Collapse consecutive stars so the recursion always advances in
            // the pattern: either consume the star and stay (text advances)
            // or drop the star and stop (match ends).
            let mut rest = pi + 1;
            while rest < pattern.len() && pattern[rest] == '*' {
                rest += 1;
            }
            match_component(pattern, text, rest, ti, seen)
                || (ti < text.len() && match_component(pattern, text, pi, ti + 1, seen))
        }
        '?' => ti < text.len() && match_component(pattern, text, pi + 1, ti + 1, seen),
        '[' => match parse_class(pattern, pi) {
            Some((next_pi, class)) => {
                ti < text.len()
                    && class(text[ti])
                    && match_component(pattern, text, next_pi, ti + 1, seen)
            }
            // Malformed class matches no character.
            None => false,
        },
        c => {
            ti < text.len() && text[ti] == c && match_component(pattern, text, pi + 1, ti + 1, seen)
        }
    }
}

/// Parse a `[...]` class starting at `open` (the `[`). Returns the pattern
/// index just past the closing `]` plus the accepted character set, or `None`
/// when unterminated.
type ClassPredicate = Box<dyn Fn(char) -> bool + 'static>;

fn parse_class(pattern: &[char], open: usize) -> Option<(usize, ClassPredicate)> {
    let mut i = open + 1;
    let negate = matches!(pattern.get(i), Some('!') | Some('^'));
    if negate {
        i += 1;
    }
    let mut items: Vec<(char, char)> = Vec::new();
    while i < pattern.len() && pattern[i] != ']' {
        if i + 2 < pattern.len() && pattern[i + 1] == '-' && pattern[i + 2] != ']' {
            items.push((pattern[i], pattern[i + 2]));
            i += 3;
        } else {
            items.push((pattern[i], pattern[i]));
            i += 1;
        }
    }
    if i >= pattern.len() {
        return None; // unterminated
    }
    let contains = move |c: char| items.iter().any(|(lo, hi)| c >= *lo && c <= *hi) != negate;
    Some((i + 1, Box::new(contains)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect(paths: &[&str], globs: &[&str]) -> Vec<String> {
        let paths: Vec<String> = paths.iter().map(|s| s.to_string()).collect();
        let globs: Vec<String> = globs.iter().map(|s| s.to_string()).collect();
        collect_files(&paths, &globs).unwrap()
    }

    #[test]
    fn literal_files_and_dirs() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), b"a").unwrap();
        std::fs::write(dir.path().join("b.txt"), b"b").unwrap();
        std::fs::write(dir.path().join(".hidden.txt"), b"h").unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub").join("c.txt"), b"c").unwrap();

        let files = collect(&[dir.path().to_str().unwrap()], &[]);
        assert_eq!(
            files.len(),
            3,
            "hidden file excluded, dir walked recursively"
        );
        assert!(files.iter().any(|f| f.ends_with("a.txt")));
        assert!(files.iter().any(|f| f.ends_with("b.txt")));
        assert!(files
            .iter()
            .any(|f| f.ends_with("sub\\c.txt") || f.ends_with("sub/c.txt")));
        assert!(!files.iter().any(|f| f.ends_with("hidden.txt")));
    }

    #[test]
    fn glob_recursive_star_star() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("one.mp4"), b"1").unwrap();
        std::fs::create_dir(dir.path().join("nested")).unwrap();
        std::fs::write(dir.path().join("nested").join("two.mp4"), b"2").unwrap();
        std::fs::write(dir.path().join("nested").join("note.txt"), b"3").unwrap();

        let pattern = format!(
            "{}/**/*.mp4",
            dir.path().to_str().unwrap().replace('\\', "/")
        );
        let files = collect(&[], &[&pattern]);
        assert_eq!(files.len(), 2, "recursive glob matched both mp4s");
    }

    #[test]
    fn glob_component_matching() {
        assert!(glob_match("*.mp4", "clip.mp4"));
        assert!(!glob_match("*.mp4", "clip.mp4.bak"));
        assert!(glob_match("videos/**/*.mp4", "videos/a/b/clip.mp4"));
        assert!(glob_match("videos/**/*.mp4", "videos/clip.mp4"));
        assert!(!glob_match("videos/**/*.mp4", "photos/clip.mp4"));
        assert!(glob_match("a?.txt", "ab.txt"));
        assert!(!glob_match("a?.txt", "abc.txt"));
    }

    #[test]
    fn glob_character_classes() {
        // Single characters.
        assert!(glob_match("[abc].txt", "a.txt"));
        assert!(glob_match("[abc].txt", "c.txt"));
        assert!(!glob_match("[abc].txt", "d.txt"));
        // Ranges.
        assert!(glob_match("[a-c].txt", "b.txt"));
        assert!(!glob_match("[a-c].txt", "d.txt"));
        // Negation via ! and ^.
        assert!(glob_match("[!abc].txt", "d.txt"));
        assert!(!glob_match("[!abc].txt", "a.txt"));
        assert!(glob_match("[^abc].txt", "z.txt"));
        // Malformed (unterminated) classes match nothing.
        assert!(!glob_match("[abc.txt", "a.txt"));
        assert!(!glob_match("[abc.txt", "abc.txt"));
    }

    #[test]
    fn duplicate_inputs_are_deduplicated() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("same.txt");
        std::fs::write(&file, b"x").unwrap();

        let one = dir.path().join("same.txt").to_string_lossy().into_owned();
        let two = one.clone();
        let files = collect(&[&one, &two], &[]);
        assert_eq!(files.len(), 1, "repeated paths must collapse to one");
    }

    #[test]
    fn long_wildcard_patterns_terminate_quickly() {
        let pattern = "*".repeat(200);
        // A wall of stars is equivalent to one star; must not blow the stack
        // or take exponential time.
        assert!(glob_match(&pattern, "aaaa"));
        assert!(glob_match(&pattern, ""));
        let mixed = format!("*a{}", "*".repeat(100));
        assert!(glob_match(&mixed, "xxxxxa"));
        assert!(!glob_match(&mixed, "xxxxx"));
    }

    #[test]
    fn unreadable_glob_base_is_an_error_not_silence() {
        let missing = std::env::temp_dir()
            .join("upio-does-not-exist-1234567890")
            .join("**/*.txt");
        // A base that cannot be walked surfaces an error instead of matching
        // zero files silently.
        let pattern = missing.to_string_lossy().into_owned();
        let result = collect_files(&[], &[pattern]);
        assert!(result.is_err(), "expected walk error for missing base");
    }
}
