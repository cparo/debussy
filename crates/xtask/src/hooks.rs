//! Installs the git hooks that run the commit gate.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

/// The pre-commit hook script. The Claude Code guard in `.claude/hooks/` looks for
/// [`HOOK_MARKER`] to decide whether the hooks are installed.
const PRE_COMMIT: &str = "#!/bin/sh\n\
    # Installed by `cargo xtask install-hooks`. Do not bypass with --no-verify.\n\
    exec cargo xtask precommit\n";

/// Text that identifies a hook installed by this tool.
pub const HOOK_MARKER: &str = "cargo xtask precommit";

/// The hooks directory git uses for this clone (honours worktrees and `core.hooksPath`).
fn hooks_dir(root: &Path) -> Result<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--git-path", "hooks"])
        .current_dir(root)
        .output()
        .context("running git rev-parse")?;
    if !output.status.success() {
        bail!("git rev-parse --git-path hooks failed: is this a git clone?");
    }
    let dir = String::from_utf8(output.stdout)?.trim().to_string();
    Ok(root.join(dir))
}

/// Writes the pre-commit hook into `dir`. An existing hook that this tool did not write is kept
/// unless `force` is set.
fn install_into(dir: &Path, force: bool) -> Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let path = dir.join("pre-commit");
    if let Ok(existing) = fs::read_to_string(&path)
        && !existing.contains(HOOK_MARKER)
        && !force
    {
        bail!(
            "{} already exists and was not written by xtask; \
             rerun with --force to replace it",
            path.display()
        );
    }
    fs::write(&path, PRE_COMMIT)?;
    make_executable(&path)?;
    Ok(path)
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    Ok(())
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<()> {
    Ok(())
}

/// Installs the hooks for the clone at `root`. Returns the pre-commit hook's path.
pub fn install(root: &Path, force: bool) -> Result<PathBuf> {
    install_into(&hooks_dir(root)?, force)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::testing::TempTree;
    use std::error::Error;

    #[test]
    fn test_install_into_writes_executable_hook() -> Result<(), Box<dyn Error>> {
        let tree = TempTree::new()?;
        let path = install_into(&tree.path().join("hooks"), false)?;
        assert!(fs::read_to_string(&path)?.contains(HOOK_MARKER));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&path)?.permissions().mode() & 0o111, 0o111);
        }
        // Reinstalling over our own hook is fine.
        install_into(&tree.path().join("hooks"), false)?;
        Ok(())
    }

    #[test]
    fn test_install_into_keeps_foreign_hook() -> Result<(), Box<dyn Error>> {
        let tree = TempTree::new()?;
        tree.write("hooks/pre-commit", "#!/bin/sh\necho mine\n")?;
        assert!(install_into(&tree.path().join("hooks"), false).is_err());
        install_into(&tree.path().join("hooks"), true)?;
        Ok(())
    }
}
