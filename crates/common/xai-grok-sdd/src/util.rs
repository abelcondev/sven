//! Shared persistence + parsing helpers for the OKF knowledge base: a light
//! frontmatter parser, slugging, sequence numbering, and artifact listing.
//! Ported from helpers in the Go `internal/sdd` package.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Parses the leading YAML frontmatter block (`--- … ---`) of a markdown file
/// into a flat key→value map. Only top-level scalar keys are read; a file with no
/// frontmatter (or an unreadable file) yields an empty map.
pub fn read_frontmatter(path: &Path) -> HashMap<String, String> {
    match fs::read_to_string(path) {
        Ok(content) => parse_frontmatter_lines(&content),
        Err(_) => HashMap::new(),
    }
}

/// Parses frontmatter from already-loaded content (the shared core of
/// [`read_frontmatter`]).
fn parse_frontmatter_lines(content: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut lines = content.split('\n');
    match lines.next() {
        Some(first) if first.trim() == "---" => {}
        _ => return out,
    }
    for line in lines {
        if line.trim() == "---" {
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim();
            if !key.is_empty() {
                out.insert(key.to_string(), value.trim().to_string());
            }
        }
    }
    out
}

/// Separates a leading YAML frontmatter block from the markdown body. Returns a
/// flat key→value map of top-level scalars and the remaining body. A document
/// without frontmatter yields an empty map and the whole content as body.
pub fn split_frontmatter(content: &str) -> (HashMap<String, String>, String) {
    let mut fm = HashMap::new();
    let normalized = content.replace("\r\n", "\n");
    if !normalized.starts_with("---\n") {
        return (fm, content.to_string());
    }
    let lines: Vec<&str> = normalized.split('\n').collect();
    let mut end: isize = -1;
    for (i, line) in lines.iter().enumerate().skip(1) {
        if line.trim() == "---" {
            end = i as isize;
            break;
        }
    }
    if end == -1 {
        return (fm, content.to_string());
    }
    let end = end as usize;
    for line in &lines[1..end] {
        if let Some((key, val)) = line.split_once(':') {
            let key = key.trim();
            if !key.is_empty() {
                fm.insert(key.to_string(), val.trim().to_string());
            }
        }
    }
    let body = lines[end + 1..].join("\n");
    let body = body.trim_start_matches('\n').to_string();
    (fm, body)
}

/// Rewrites the `status:` line inside a markdown file's frontmatter in place,
/// preserving every other line. Errors if the file has no frontmatter or no
/// status field.
pub fn set_frontmatter_status(path: &Path, status: &str) -> anyhow::Result<()> {
    let raw = fs::read_to_string(path)?;
    let normalized = raw.replace("\r\n", "\n");
    let mut lines: Vec<String> = normalized.split('\n').map(String::from).collect();
    if lines.is_empty() || lines[0].trim() != "---" {
        anyhow::bail!("no frontmatter in {}", path.display());
    }
    for i in 1..lines.len() {
        if lines[i].trim() == "---" {
            break;
        }
        if lines[i].trim_start().starts_with("status:") {
            lines[i] = format!("status: {status}");
            fs::write(path, lines.join("\n"))?;
            return Ok(());
        }
    }
    anyhow::bail!("no status field in {}", path.display())
}

/// Returns the next zero-padded 3-digit sequence for `dir`, based on the highest
/// `NNN-` prefix among its `*.md` files. An empty or missing dir yields `"001"`.
pub fn next_number(dir: &Path) -> anyhow::Result<String> {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok("001".to_string()),
        Err(e) => return Err(e.into()),
    };
    let mut highest = 0u32;
    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let digits: String = name.chars().take_while(|c| c.is_ascii_digit()).collect();
        if digits.is_empty() {
            continue;
        }
        if let Ok(n) = digits.parse::<u32>()
            && n > highest
        {
            highest = n;
        }
    }
    Ok(format!("{:03}", highest + 1))
}

/// Lowercases `title` and reduces it to a filename-safe `[a-z0-9-]` slug.
pub fn slugify(title: &str) -> String {
    let mut b = String::new();
    let mut prev_dash = false;
    for r in title.trim().to_lowercase().chars() {
        if r.is_ascii_lowercase() || r.is_ascii_digit() {
            b.push(r);
            prev_dash = false;
        } else if (r == ' ' || r == '-' || r == '_' || r == '/' || r == '.')
            && !b.is_empty()
            && !prev_dash
        {
            b.push('-');
            prev_dash = true;
        }
    }
    let out = b.trim_matches('-').to_string();
    if out.is_empty() {
        return "untitled".to_string();
    }
    out
}

/// Appends `line` + `"\n"` to `path`, creating it if absent.
pub fn append_line(path: &Path, line: &str) -> anyhow::Result<()> {
    let mut f = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)?;
    f.write_all(line.as_bytes())?;
    f.write_all(b"\n")?;
    Ok(())
}

/// Returns the `*.md` files in `dir`, excluding template and hidden files (names
/// beginning with `_` or `.`). A missing dir yields an empty vec. Sorted by path.
pub fn list_artifacts(dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut out = Vec::new();
    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('_') || name.starts_with('.') || !name.ends_with(".md") {
            continue;
        }
        out.push(dir.join(name));
    }
    out.sort();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_cases() {
        let cases = [
            ("Magic-link auth", "magic-link-auth"),
            ("  Trim  Me  ", "trim-me"),
            ("Weird!!!Chars@@@Here", "weirdcharshere"),
            ("003-already/slugged.md", "003-already-slugged-md"),
            ("", "untitled"),
            ("---", "untitled"),
        ];
        for (input, want) in cases {
            assert_eq!(slugify(input), want, "slugify({input:?})");
        }
    }
}
