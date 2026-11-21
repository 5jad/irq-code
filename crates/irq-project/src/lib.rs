//! Project loading for a deliberately small, dependency-free `irq.toml` profile.
//!
//! The accepted manifest is TOML-shaped but intentionally restricted to a single
//! `[package]` table with `name` and `sources` keys. Unknown syntax is rejected,
//! never ignored. The parser is not a general-purpose TOML implementation.

use std::{
    collections::HashSet,
    fmt, fs,
    path::{Component, Path, PathBuf},
};

use irq_ast::Module;

pub const MANIFEST_FILE: &str = "irq.toml";
pub const MAX_PROJECT_FILES: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectManifest {
    pub name: String,
    pub sources: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedModule {
    pub relative_path: PathBuf,
    pub module: Module,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub root: PathBuf,
    pub manifest: ProjectManifest,
    pub modules: Vec<LoadedModule>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectError {
    pub code: &'static str,
    pub message: String,
    pub relative_path: Option<PathBuf>,
    pub line: Option<usize>,
    pub span: Option<irq_lexer::Span>,
}

impl fmt::Display for ProjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for ProjectError {}

fn error(code: &'static str, message: impl Into<String>) -> ProjectError {
    ProjectError {
        code,
        message: message.into(),
        relative_path: None,
        line: None,
        span: None,
    }
}

/// Parse the intentionally restricted manifest grammar documented above.
pub fn parse_manifest(source: &str) -> Result<ProjectManifest, ProjectError> {
    let mut in_package = false;
    let mut saw_package = false;
    let mut name = None;
    let mut sources = None;
    for (line_no, raw) in source.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "[package]" {
            if saw_package {
                return Err(error("IRQ-PROJECT-MANIFEST", "duplicate [package] table"));
            }
            saw_package = true;
            in_package = true;
            continue;
        }
        if !in_package {
            return Err(error(
                "IRQ-PROJECT-MANIFEST",
                format!("unsupported manifest syntax on line {}", line_no + 1),
            ));
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(error(
                "IRQ-PROJECT-MANIFEST",
                format!("expected key = value on line {}", line_no + 1),
            ));
        };
        let key = key.trim();
        let value = value.trim();
        match key {
            "name" => {
                if name.is_some() {
                    return Err(error("IRQ-PROJECT-MANIFEST", "duplicate package.name"));
                }
                let value = parse_quoted(value).ok_or_else(|| {
                    error(
                        "IRQ-PROJECT-MANIFEST",
                        "package.name must be a quoted string",
                    )
                })?;
                if !valid_identifier(value) {
                    return Err(error(
                        "IRQ-PROJECT-MANIFEST",
                        "package.name must be an ASCII identifier",
                    ));
                }
                name = Some(value.to_owned());
            }
            "sources" => {
                if sources.is_some() {
                    return Err(error("IRQ-PROJECT-MANIFEST", "duplicate package.sources"));
                }
                sources = Some(parse_string_array(value)?);
            }
            _ => {
                return Err(error(
                    "IRQ-PROJECT-MANIFEST",
                    format!("unknown package key `{key}`"),
                ));
            }
        }
    }
    if !saw_package {
        return Err(error("IRQ-PROJECT-MANIFEST", "missing [package] table"));
    }
    let name = name.ok_or_else(|| error("IRQ-PROJECT-MANIFEST", "missing package.name"))?;
    let sources =
        sources.ok_or_else(|| error("IRQ-PROJECT-MANIFEST", "missing package.sources"))?;
    if sources.is_empty() {
        return Err(error(
            "IRQ-PROJECT-MANIFEST",
            "package.sources must not be empty",
        ));
    }
    if sources.len() > MAX_PROJECT_FILES {
        return Err(error(
            "IRQ-PROJECT-LIMIT",
            format!("project exceeds the {MAX_PROJECT_FILES} source file limit"),
        ));
    }
    let mut unique = HashSet::new();
    for source in &sources {
        if source.is_absolute()
            || source
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err(error(
                "IRQ-PROJECT-PATH",
                format!(
                    "source path `{}` must be relative and cannot contain `.` or `..`",
                    source.display()
                ),
            ));
        }
        if source.extension().is_none_or(|ext| ext != "irq") {
            return Err(error(
                "IRQ-PROJECT-PATH",
                format!(
                    "source path `{}` must have the .irq extension",
                    source.display()
                ),
            ));
        }
        if !unique.insert(source.clone()) {
            return Err(error(
                "IRQ-PROJECT-MANIFEST",
                format!("duplicate source path `{}`", source.display()),
            ));
        }
    }
    Ok(ProjectManifest { name, sources })
}

/// Load and parse every explicitly listed source in deterministic manifest order.
pub fn load_project(root: impl AsRef<Path>) -> Result<Project, ProjectError> {
    let requested_root = root.as_ref();
    let root = fs::canonicalize(requested_root).map_err(|e| {
        error(
            "IRQ-PROJECT-IO",
            format!(
                "cannot resolve project root `{}`: {e}",
                requested_root.display()
            ),
        )
    })?;
    if !root.is_dir() {
        return Err(error("IRQ-PROJECT-IO", "project root is not a directory"));
    }
    let manifest_path = root.join(MANIFEST_FILE);
    let text = fs::read_to_string(&manifest_path).map_err(|e| {
        error(
            "IRQ-PROJECT-IO",
            format!("cannot read `{}`: {e}", manifest_path.display()),
        )
    })?;
    let manifest = parse_manifest(&text)?;
    let mut modules = Vec::with_capacity(manifest.sources.len());
    let mut names = HashSet::new();
    for relative_path in &manifest.sources {
        let requested = root.join(relative_path);
        let canonical = fs::canonicalize(&requested).map_err(|e| {
            error(
                "IRQ-PROJECT-IO",
                format!("cannot resolve source `{}`: {e}", relative_path.display()),
            )
        })?;
        if !canonical.starts_with(&root) {
            return Err(error(
                "IRQ-PROJECT-PATH",
                format!(
                    "source `{}` resolves outside the project root",
                    relative_path.display()
                ),
            ));
        }
        if !canonical.is_file() {
            return Err(error(
                "IRQ-PROJECT-IO",
                format!("source `{}` is not a regular file", relative_path.display()),
            ));
        }
        let metadata = fs::metadata(&canonical).map_err(|e| {
            error(
                "IRQ-PROJECT-IO",
                format!("cannot inspect source `{}`: {e}", relative_path.display()),
            )
        })?;
        if metadata.len() > irq_lexer::MAX_SOURCE_BYTES as u64 {
            return Err(error(
                "IRQ-PROJECT-LIMIT",
                format!(
                    "source `{}` exceeds the {} byte limit",
                    relative_path.display(),
                    irq_lexer::MAX_SOURCE_BYTES
                ),
            ));
        }
        let source = fs::read_to_string(&canonical).map_err(|e| {
            error(
                "IRQ-PROJECT-IO",
                format!("cannot read source `{}`: {e}", relative_path.display()),
            )
        })?;
        let module = irq_parser::parse(&source).map_err(|e| ProjectError {
            code: e.code,
            message: e.message,
            relative_path: Some(relative_path.clone()),
            line: Some(e.line),
            span: e.span,
        })?;
        if !names.insert(module.name.clone()) {
            return Err(error(
                "IRQ-PROJECT-DUPLICATE-MODULE",
                format!("duplicate module name `{}`", module.name),
            ));
        }
        modules.push(LoadedModule {
            relative_path: relative_path.clone(),
            module,
        });
    }
    Ok(Project {
        root,
        manifest,
        modules,
    })
}

fn parse_quoted(value: &str) -> Option<&str> {
    value
        .strip_prefix('"')?
        .strip_suffix('"')
        .filter(|s| !s.contains('"') && !s.contains('\\'))
}

fn parse_string_array(value: &str) -> Result<Vec<PathBuf>, ProjectError> {
    let inner = value
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or_else(|| {
            error(
                "IRQ-PROJECT-MANIFEST",
                "package.sources must be a single-line array of quoted paths",
            )
        })?;
    let mut result = Vec::new();
    for part in inner.split(',') {
        let item = part.trim();
        if item.is_empty() {
            continue;
        }
        let path = parse_quoted(item).ok_or_else(|| {
            error(
                "IRQ-PROJECT-MANIFEST",
                "source entries must be quoted strings without escapes",
            )
        })?;
        if path.is_empty() {
            return Err(error("IRQ-PROJECT-PATH", "source path must not be empty"));
        }
        result.push(PathBuf::from(path));
    }
    Ok(result)
}

fn valid_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    chars
        .next()
        .is_some_and(|c| c == '_' || c.is_ascii_alphabetic())
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> PathBuf {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("irq-project-test-{}-{id}", std::process::id()));
        fs::create_dir_all(path.join("src")).unwrap();
        path
    }
    fn source(module: &str) -> String {
        format!(
            "mod {module}\n\nfn run() -> i32\n  reads: none\n  writes: none\n  requires: true\n  ensures: true\n  body:\n    return 1\n"
        )
    }

    #[test]
    fn parses_manifest_and_rejects_unknown_or_unsafe_entries() {
        let parsed = parse_manifest(
            "[package]\nname = \"demo\"\nsources = [\"src/main.irq\", \"src/lib.irq\"]\n",
        )
        .unwrap();
        assert_eq!(parsed.name, "demo");
        assert_eq!(parsed.sources.len(), 2);
        for bad in [
            "../escape.irq",
            "/tmp/file.irq",
            "src\\\\escape.irq",
            "src/main.rs",
        ] {
            let manifest = format!("[package]\nname = \"demo\"\nsources = [\"{bad}\"]\n");
            assert!(parse_manifest(&manifest).is_err(), "accepted {bad}");
        }
        assert!(
            parse_manifest("[package]\nname = \"demo\"\nsources = []\nunknown = true\n").is_err()
        );
        assert!(
            parse_manifest(
                "[package]\nname = \"demo\"\nname = \"other\"\nsources = [\"src/a.irq\"]"
            )
            .is_err()
        );
        assert!(
            parse_manifest("[package]\nname = \"demo\"\nsources = [\"src/a.irq\", \"src/a.irq\"]")
                .is_err()
        );
        assert!(parse_manifest("[package]\nname = \"démo\"\nsources = [\"src/a.irq\"]").is_err());
    }

    #[test]
    fn loads_files_in_manifest_order_and_rejects_duplicate_module_names() {
        let root = temp_dir();
        fs::write(
            root.join("irq.toml"),
            "[package]\nname = \"demo\"\nsources = [\"src/a.irq\", \"src/b.irq\"]\n",
        )
        .unwrap();
        fs::write(root.join("src/a.irq"), source("alpha")).unwrap();
        fs::write(root.join("src/b.irq"), source("beta")).unwrap();
        let project = load_project(&root).unwrap();
        assert_eq!(
            project
                .modules
                .iter()
                .map(|m| m.module.name.as_str())
                .collect::<Vec<_>>(),
            ["alpha", "beta"]
        );
        fs::write(root.join("src/b.irq"), source("alpha")).unwrap();
        assert_eq!(
            load_project(&root).unwrap_err().code,
            "IRQ-PROJECT-DUPLICATE-MODULE"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinks_that_escape_project_root() {
        use std::os::unix::fs::symlink;
        let root = temp_dir();
        let outside = root.with_extension("outside.irq");
        fs::write(&outside, source("escape")).unwrap();
        symlink(&outside, root.join("src/link.irq")).unwrap();
        fs::write(
            root.join("irq.toml"),
            "[package]\nname = \"demo\"\nsources = [\"src/link.irq\"]\n",
        )
        .unwrap();
        assert_eq!(load_project(&root).unwrap_err().code, "IRQ-PROJECT-PATH");
        fs::remove_file(root.join("src/link.irq")).unwrap();
        fs::remove_dir_all(root).unwrap();
        fs::remove_file(outside).unwrap();
    }
}
