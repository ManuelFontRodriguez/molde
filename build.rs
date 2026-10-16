//! Stamps the commit a binary was built from into `molde --version`, so a bug
//! report names the exact source. Only molde's own repository counts: a crates.io
//! package or a tarball unpacked inside some other git repository prints the
//! package version alone, never that repository's commit.
use std::path::{Path, PathBuf};
use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    Command::new("git")
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let own_repo = git(&["rev-parse", "--show-toplevel"])
        .and_then(|top| Path::new(&top).canonicalize().ok())
        .zip(manifest_dir.canonicalize().ok())
        .is_some_and(|(top, here)| top == here);
    let version = match own_repo
        .then(|| git(&["rev-parse", "--short=10", "HEAD"]))
        .flatten()
    {
        Some(sha) => {
            let dirty = git(&["status", "--porcelain", "--untracked-files=no"]).is_some();
            let date = git(&["show", "-s", "--format=%cs", "HEAD"]).unwrap_or_default();
            let sha = if dirty { format!("{sha}-dirty") } else { sha };
            format!("{} ({sha} {date})", env!("CARGO_PKG_VERSION"))
        }
        None => env!("CARGO_PKG_VERSION").to_string(),
    };
    println!("cargo:rustc-env=MOLDE_VERSION={version}");
    if !own_repo {
        return;
    }
    // rerun when HEAD moves (a commit, a checkout) or the index changes; --git-path
    // resolves these in a worktree too, where .git is a file. Source edits do not
    // rerun this script, so `-dirty` refreshes only when the index does.
    println!("cargo:rerun-if-changed=build.rs");
    let mut watch = vec!["HEAD".to_string(), "index".to_string()];
    if let Some(head_ref) = git(&["symbolic-ref", "-q", "HEAD"]) {
        watch.push(head_ref);
    }
    watch.push("packed-refs".to_string());
    for name in watch {
        if let Some(path) = git(&["rev-parse", "--git-path", &name]) {
            if Path::new(&path).exists() {
                println!("cargo:rerun-if-changed={path}");
            }
        }
    }
}
