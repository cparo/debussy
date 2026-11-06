//! Locating the repository, walking its files, and reporting problems found in them.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// The repository root: two levels above this crate's manifest.
pub fn root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .ancestors()
        .nth(2)
        .map_or_else(|| manifest.to_path_buf(), Path::to_path_buf)
}

/// A problem found by a check, located at a file (relative to the root) and optionally a line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// Path relative to the repository root, with `/` separators.
    pub path: String,
    /// 1-based line number, when the problem has one.
    pub line: Option<usize>,
    /// What is wrong and how to fix it.
    pub message: String,
}

impl Problem {
    /// A problem at a specific line of a file.
    pub fn at(path: &str, line: usize, message: impl Into<String>) -> Self {
        Self {
            path: path.to_string(),
            line: Some(line),
            message: message.into(),
        }
    }

    /// A problem with a whole file.
    pub fn in_file(path: &str, message: impl Into<String>) -> Self {
        Self {
            path: path.to_string(),
            line: None,
            message: message.into(),
        }
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(f, "{}:{}: {}", self.path, line, self.message),
            None => write!(f, "{}: {}", self.path, self.message),
        }
    }
}

/// Directories never scanned: build output, version control, and dependencies.
const SKIPPED_DIRS: &[&str] = &[".git", "target", "node_modules"];

/// Hidden directories that are scanned despite their leading dot.
const SCANNED_HIDDEN_DIRS: &[&str] = &[".claude", ".github", ".cargo"];

/// Returns every regular file under `root` (relative, `/`-separated, sorted), skipping build
/// output, version control, and hidden directories other than the project's own configuration.
pub fn files(root: &Path) -> Result<Vec<String>> {
    let mut found = Vec::new();
    walk(root, root, &mut found)?;
    found.sort();
    Ok(found)
}

fn walk(root: &Path, dir: &Path, found: &mut Vec<String>) -> Result<()> {
    let entries = fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))?;
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        let kind = entry.file_type()?;
        if kind.is_dir() {
            let hidden = name.starts_with('.') && !SCANNED_HIDDEN_DIRS.contains(&name.as_str());
            if !hidden && !SKIPPED_DIRS.contains(&name.as_str()) {
                walk(root, &path, found)?;
            }
        } else if kind.is_file() {
            found.push(relative(root, &path));
        }
    }
    Ok(())
}

/// `path` relative to `root`, with `/` separators.
pub fn relative(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Reads a file under `root` as UTF-8.
pub fn read(root: &Path, rel: &str) -> Result<String> {
    let path = root.join(rel);
    fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))
}

/// Resolves a relative link `target` (no anchor) written in the file `from`, both relative to the
/// repository root, normalising `.` and `..`. Returns `None` if it escapes the root.
pub fn resolve(from: &str, target: &str) -> Option<String> {
    let mut parts: Vec<&str> = from.split('/').collect();
    parts.pop();
    for part in target.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    Some(parts.join("/"))
}

#[cfg(test)]
pub mod testing {
    //! A throwaway directory tree for tests.

    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    /// A temporary directory removed when dropped.
    pub struct TempTree {
        path: PathBuf,
    }

    impl TempTree {
        /// Creates an empty directory unique to this process and call.
        pub fn new() -> std::io::Result<Self> {
            let n = COUNTER.fetch_add(1, Ordering::SeqCst);
            let path = std::env::temp_dir().join(format!("xtask-test-{}-{n}", std::process::id()));
            if path.exists() {
                fs::remove_dir_all(&path)?;
            }
            fs::create_dir_all(&path)?;
            Ok(Self { path })
        }

        /// The directory's path.
        pub fn path(&self) -> &Path {
            &self.path
        }

        /// Writes `contents` to `rel`, creating parent directories.
        pub fn write(&self, rel: &str, contents: &str) -> std::io::Result<()> {
            let path = self.path.join(rel);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(path, contents)
        }
    }

    impl Drop for TempTree {
        fn drop(&mut self) {
            // Best effort: a leftover temp directory is harmless.
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::TempTree;
    use super::*;
    use std::error::Error;

    #[test]
    fn test_resolve_handles_parent_segments() {
        assert_eq!(
            resolve("docs/adrs/0001-x.md", "../architecture/a.md"),
            Some("docs/architecture/a.md".to_string())
        );
        assert_eq!(
            resolve("README.md", "./docs/x.md"),
            Some("docs/x.md".to_string())
        );
        assert_eq!(resolve("README.md", "../outside.md"), None);
    }

    #[test]
    fn test_files_skips_target_and_hidden_dirs() -> Result<(), Box<dyn Error>> {
        let tree = TempTree::new()?;
        tree.write("a.md", "")?;
        tree.write("target/debug/x", "")?;
        tree.write(".git/HEAD", "")?;
        tree.write(".claude/skills/s/SKILL.md", "")?;
        tree.write(".hidden/x", "")?;
        assert_eq!(
            files(tree.path())?,
            vec![".claude/skills/s/SKILL.md".to_string(), "a.md".to_string()]
        );
        Ok(())
    }
}
