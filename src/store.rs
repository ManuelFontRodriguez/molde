use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// One writer per out dir: `serve --watch`, `events`, the MCP `check`
/// tool and a hand-run `check` or `build` on the same `--out` take turns for
/// the whole render -> check -> publish sequence, so dedup never reads a build
/// another process is writing and a slower, older build can never publish over
/// a newer one. Readers (`diff`, `serve`) don't lock. Released on drop.
/// Not re-entrant: the lock is per open file, so calling `build` or `run_check`
/// while holding the guard deadlocks, even in the same process. It has no timeout:
/// a second writer waits for as long as the first one runs.
pub fn lock_out(out: &Path) -> Result<fs::File> {
    fs::create_dir_all(out).with_context(|| format!("creating {}", out.display()))?;
    let path = out.join(".molde.lock");
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .with_context(|| format!("opening {}", path.display()))?;
    file.lock()
        .with_context(|| format!("locking {}", path.display()))?;
    Ok(file)
}

/// Atomic swap: current -> builds/<id> (tmp symlink + rename).
pub fn publish(out: &Path, build_dir: &Path) -> Result<()> {
    let build_id = build_dir
        .file_name()
        .context("build dir has no name")?
        .to_string_lossy()
        .into_owned();
    let current = out.join("current");
    let tmp = out.join(format!(".current-{build_id}"));
    let target = PathBuf::from("builds").join(&build_id);
    // a run killed between symlink and rename leaves the temp link behind, and
    // creating it again would fail every later publish of this build
    match fs::remove_file(&tmp) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
        _ => {}
    }
    std::os::unix::fs::symlink(&target, &tmp)?;
    fs::rename(&tmp, &current)?;
    Ok(())
}

/// Build ids under out/builds, oldest first.
pub fn list_builds(out: &Path) -> Result<Vec<String>> {
    let mut ids: Vec<String> = fs::read_dir(out.join("builds"))
        .context("no builds yet")?
        .filter_map(|e| e.ok())
        // file_type, not path().is_dir(): a symlink placed in builds/ is never a
        // build, so diff cannot be pointed through one at a directory elsewhere
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        // dot-prefixed: a build still being written (or abandoned by a crash)
        .filter(|id| !id.starts_with('.'))
        .collect();
    ids.sort_by_key(|s| s.parse::<u128>().unwrap_or(0));
    Ok(ids)
}

/// A `.tmp-<id>` build folder this old belongs to a render that died; younger
/// ones may still be being written by another process.
const ABANDONED_TMP_AGE: std::time::Duration = std::time::Duration::from_secs(3600);

/// Delete all but the `keep` newest builds. The published build is
/// always kept, whatever its age, and render folders abandoned by a crash
/// (`.tmp-*`, older than an hour) are swept. Only real directories directly
/// under `out/builds` with an all-digit build id are deleted — never a
/// symlink's target, never a folder someone else put there.
pub fn gc_builds(out: &Path, keep: usize) -> Result<serde_json::Value> {
    if keep == 0 {
        bail!("--keep must be at least 1: the newest build is the diff baseline");
    }
    let builds = out.join("builds");
    if !builds.is_dir() {
        bail!("no builds yet in {}", out.display());
    }
    // a writer's dedup compares against the newest and published builds; deleting
    // under it would race, so gc waits its turn like any writer
    let _lock = lock_out(out)?;
    let ids: Vec<String> = list_builds(out)?
        .into_iter()
        // build ids are nanosecond timestamps: anything else someone put in
        // builds/ (notes, a backup) is not ours to delete
        .filter(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
        .filter(|id| fs::symlink_metadata(builds.join(id)).is_ok_and(|m| m.is_dir()))
        .collect();
    let published = fs::read_link(out.join("current"))
        .ok()
        .and_then(|t| t.file_name().map(|n| n.to_string_lossy().into_owned()));
    let newest = &ids[ids.len().saturating_sub(keep)..];
    let mut kept = Vec::new();
    let mut removed = Vec::new();
    for id in &ids {
        if newest.contains(id) || published.as_ref() == Some(id) {
            kept.push(id.clone());
            continue;
        }
        let dir = builds.join(id);
        fs::remove_dir_all(&dir).with_context(|| format!("removing {}", dir.display()))?;
        removed.push(id.clone());
    }
    let mut abandoned = Vec::new();
    for entry in fs::read_dir(&builds)?.filter_map(|e| e.ok()) {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(meta) = entry.metadata() else { continue };
        let old = meta
            .modified()
            .ok()
            .and_then(|m| m.elapsed().ok())
            .is_some_and(|age| age > ABANDONED_TMP_AGE);
        if name.starts_with(".tmp-") && entry.file_type().is_ok_and(|t| t.is_dir()) && old {
            fs::remove_dir_all(entry.path())?;
            abandoned.push(name);
        }
    }
    Ok(serde_json::json!({
        "kept": kept,
        "removed": removed,
        "abandoned_removed": abandoned,
    }))
}
