use std::process::Command;

pub fn current_branch() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;

    if output.status.success() {
        let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if branch.is_empty() || branch == "HEAD" {
            None
        } else {
            Some(branch)
        }
    } else {
        None
    }
}

pub fn head_sha() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;

    if output.status.success() {
        let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if sha.is_empty() {
            None
        } else {
            Some(sha)
        }
    } else {
        None
    }
}

/// Extract the repo name from the git remote origin URL, or fall back to the directory name.
pub fn repo_name() -> Option<String> {
    // Try to get the remote origin URL
    let output = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .output()
        .ok()?;

    if output.status.success() {
        let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let clean = url.trim_end_matches(".git");
        // Parse repo name from URL patterns:
        // git@github.com:org/repo.git -> org/repo
        // https://github.com/org/repo.git -> org/repo
        let name = if clean.contains("://") {
            // HTTPS URL: take last two path segments (org/repo)
            let parts: Vec<&str> = clean.rsplitn(3, '/').collect();
            if parts.len() >= 2 {
                Some(format!("{}/{}", parts[1], parts[0]))
            } else {
                None
            }
        } else {
            // SSH URL: git@host:org/repo
            clean.rsplit_once(':').map(|(_, path)| path.to_string())
        };
        if name.is_some() {
            return name;
        }
    }

    // Fall back to directory name from repo root
    repo_root().map(|root| {
        std::path::Path::new(&root)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or(root)
    })
}

pub fn repo_root() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()?;

    if output.status.success() {
        let root = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if root.is_empty() {
            None
        } else {
            Some(root)
        }
    } else {
        None
    }
}
