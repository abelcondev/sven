//! Compiled, default-on code-quality gate. The flagship gate caps source-file
//! length as a ratchet: it blocks creating a file over the limit or growing a
//! file past it, but still lets you edit an already-oversized file as long as the
//! change does not add lines. Ported from the Go `internal/qualitygate`.

use std::path::Path;

/// The default source-file line cap.
pub const DEFAULT_MAX_LINES: i64 = 300;

/// Controls the line-length gate.
#[derive(Debug, Clone, Copy)]
pub struct Config {
    pub enabled: bool,
    pub max_lines: i64,
}

/// Resolves the gate configuration from the environment:
///
/// - `KEZ_QUALITY_GATES=off|0|false` disables all quality gates
/// - `KEZ_MAX_FILE_LINES=<n>` overrides the line cap (`<=0` also disables)
///
/// `getenv` is injected for testability; pass an `std::env::var`-backed closure
/// in production.
pub fn config_from_env(getenv: impl Fn(&str) -> String) -> Config {
    let mut cfg = Config {
        enabled: true,
        max_lines: DEFAULT_MAX_LINES,
    };
    match getenv("KEZ_QUALITY_GATES").trim().to_lowercase().as_str() {
        "off" | "0" | "false" | "no" => cfg.enabled = false,
        _ => {}
    }
    let raw = getenv("KEZ_MAX_FILE_LINES");
    let raw = raw.trim();
    if !raw.is_empty()
        && let Ok(n) = raw.parse::<i64>()
    {
        cfg.max_lines = n;
        if n <= 0 {
            cfg.enabled = false;
        }
    }
    cfg
}

/// Returned when a write is blocked by the quality gate. Its message is
/// model-facing: it explains the violation and how to proceed.
#[derive(Debug, Clone)]
pub struct GateError {
    pub path: String,
    pub lines: i64,
    pub max: i64,
}

impl std::fmt::Display for GateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Error: write blocked by quality gate — {} would be {} lines (max {}). \
             Split it into smaller files or modules, or move code into a new file, then retry. \
             (Override with KEZ_MAX_FILE_LINES=<n>, or disable with KEZ_QUALITY_GATES=off.)",
            self.path, self.lines, self.max
        )
    }
}

impl std::error::Error for GateError {}

/// Enforces the line-length gate for a write of `new_content` to `path`, given
/// the file's `prior_content` (`""` for a new file). Reads its configuration from
/// the process environment. Returns `Ok(())` when the write is allowed.
pub fn check(path: &str, prior_content: &str, new_content: &str) -> Result<(), GateError> {
    check_with_config(
        path,
        prior_content,
        new_content,
        config_from_env(|k| std::env::var(k).unwrap_or_default()),
    )
}

/// The testable core of [`check`].
pub fn check_with_config(
    path: &str,
    prior_content: &str,
    new_content: &str,
    cfg: Config,
) -> Result<(), GateError> {
    if !cfg.enabled || cfg.max_lines <= 0 {
        return Ok(());
    }
    if !is_gated_source_file(path) {
        return Ok(());
    }
    let next = count_lines(new_content);
    if next <= cfg.max_lines {
        return Ok(());
    }
    // Ratchet: an already-oversized file may still be edited as long as the
    // change does not add lines.
    let prior = count_lines(prior_content);
    if prior > cfg.max_lines && next <= prior {
        return Ok(());
    }
    Err(GateError {
        path: path.to_string(),
        lines: next,
        max: cfg.max_lines,
    })
}

/// Counts lines the way an editor does: newline-separated lines, counting a final
/// line with no trailing newline. Empty content is zero lines.
fn count_lines(content: &str) -> i64 {
    if content.is_empty() {
        return 0;
    }
    let mut n = content.matches('\n').count() as i64;
    if !content.ends_with('\n') {
        n += 1;
    }
    n
}

/// Hand-authored source-code extensions the length gate applies to. Data, docs,
/// config, and lock files are intentionally absent.
const GATED_EXTENSIONS: &[&str] = &[
    "go", "ts", "tsx", "js", "jsx", "mjs", "cjs", "py", "rs", "java", "kt", "kts", "rb", "php",
    "c", "h", "cc", "cpp", "hpp", "cs", "swift", "scala", "svelte", "vue",
];

/// Files that are machine-generated even though they carry a gated extension.
const GENERATED_MARKERS: &[&str] = &[
    ".pb.go",
    ".gen.go",
    "_templ.go",
    ".generated.",
    "_generated.",
    ".min.js",
    ".min.css",
];

/// Reports whether `path` is a hand-authored source file that per-file gates apply
/// to. Shared with the branch guard so both agree on what "code" means.
pub fn is_gated_source_file(path: &str) -> bool {
    let base = Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let ext = base.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    if !GATED_EXTENSIONS.contains(&ext) {
        return false;
    }
    for marker in GENERATED_MARKERS {
        if base.contains(marker) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(n: usize) -> String {
        "x\n".repeat(n)
    }

    #[test]
    fn check_with_config_cases() {
        let cfg = Config {
            enabled: true,
            max_lines: 300,
        };
        let cases: &[(&str, &str, String, String, bool)] = &[
            (
                "new small file ok",
                "a.go",
                String::new(),
                lines(299),
                false,
            ),
            (
                "new file at limit ok",
                "a.go",
                String::new(),
                lines(300),
                false,
            ),
            (
                "new file over limit blocked",
                "a.go",
                String::new(),
                lines(301),
                true,
            ),
            (
                "grow existing across limit blocked",
                "a.go",
                lines(250),
                lines(320),
                true,
            ),
            (
                "edit oversized without growing ok",
                "a.go",
                lines(400),
                lines(380),
                false,
            ),
            (
                "edit oversized down to equal ok",
                "a.go",
                lines(400),
                lines(400),
                false,
            ),
            (
                "grow an already-oversized file blocked",
                "a.go",
                lines(400),
                lines(420),
                true,
            ),
            (
                "non-code extension ignored",
                "README.md",
                String::new(),
                lines(900),
                false,
            ),
            (
                "json data ignored",
                "data.json",
                String::new(),
                lines(900),
                false,
            ),
            (
                "generated go ignored",
                "api.pb.go",
                String::new(),
                lines(900),
                false,
            ),
            (
                "minified js ignored",
                "bundle.min.js",
                String::new(),
                lines(900),
                false,
            ),
            (
                "svelte gated",
                "App.svelte",
                String::new(),
                lines(400),
                true,
            ),
        ];
        for (name, path, prior, next, blocked) in cases {
            let res = check_with_config(path, prior, next, cfg);
            assert_eq!(res.is_err(), *blocked, "{name}: {path}");
        }
    }

    #[test]
    fn disabled_or_zero_max_allows() {
        assert!(
            check_with_config(
                "a.go",
                "",
                &lines(9000),
                Config {
                    enabled: false,
                    max_lines: 300
                }
            )
            .is_ok()
        );
        assert!(
            check_with_config(
                "a.go",
                "",
                &lines(9000),
                Config {
                    enabled: true,
                    max_lines: 0
                }
            )
            .is_ok()
        );
    }

    #[test]
    fn config_from_env_cases() {
        let env = |m: &[(&str, &str)]| {
            let m: std::collections::HashMap<String, String> = m
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            move |k: &str| m.get(k).cloned().unwrap_or_default()
        };
        let c = config_from_env(env(&[]));
        assert!(c.enabled && c.max_lines == DEFAULT_MAX_LINES);
        assert!(!config_from_env(env(&[("KEZ_QUALITY_GATES", "off")])).enabled);
        assert_eq!(
            config_from_env(env(&[("KEZ_MAX_FILE_LINES", "120")])).max_lines,
            120
        );
        assert!(!config_from_env(env(&[("KEZ_MAX_FILE_LINES", "0")])).enabled);
    }

    #[test]
    fn gate_error_message_guides_the_model() {
        let err = check_with_config(
            "internal/big.go",
            "",
            &lines(500),
            Config {
                enabled: true,
                max_lines: 300,
            },
        )
        .unwrap_err();
        let msg = err.to_string();
        for want in [
            "internal/big.go",
            "500",
            "300",
            "Split",
            "KEZ_MAX_FILE_LINES",
        ] {
            assert!(msg.contains(want), "gate message missing {want:?}: {msg}");
        }
    }
}
