//! The safety net: hash and copy the real configuration files a harness might
//! touch before it runs, then restore any that changed once it exits.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use time::macros::format_description;

pub struct Guard {
    /// Absolute paths of the protected files.
    paths: Vec<PathBuf>,
    /// The directory this launch's copies live in.
    pub snapshot: PathBuf,
    /// Root the relative snapshot layout is computed against.
    root: PathBuf,
    /// Hash of every protected file that existed before launch.
    before: BTreeMap<PathBuf, String>,
    keep: usize,
}

pub struct Outcome {
    pub restored: Vec<PathBuf>,
    pub created: Vec<PathBuf>,
    pub verified: usize,
}

impl Guard {
    /// `protected` are paths relative to `root` (the home directory in real
    /// use); `snapshots` is the directory that holds one subdirectory per launch.
    pub fn new(
        root: &Path,
        snapshots: &Path,
        harness: &str,
        protected: &[&str],
        keep: usize,
    ) -> Self {
        let stamp = time::OffsetDateTime::now_utc()
            .format(format_description!(
                "[year][month][day]-[hour][minute][second]"
            ))
            .unwrap_or_else(|_| "now".into());
        Self {
            paths: protected.iter().map(|p| root.join(p)).collect(),
            snapshot: snapshots.join(format!("{stamp}-{harness}")),
            root: root.to_path_buf(),
            before: BTreeMap::new(),
            keep,
        }
    }

    /// Copy every protected file that exists into the snapshot and remember its hash.
    pub fn take(&mut self) -> Result<()> {
        std::fs::create_dir_all(&self.snapshot)?;
        for path in &self.paths {
            if !path.is_file() {
                continue;
            }
            let digest = sha256(path)?;
            let copy = self.copy_path(path);
            if let Some(parent) = copy.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(path, &copy)
                .with_context(|| format!("snapshotting {}", path.display()))?;
            self.before.insert(path.clone(), digest);
        }
        let manifest: BTreeMap<String, &String> = self
            .before
            .iter()
            .map(|(p, h)| (p.display().to_string(), h))
            .collect();
        std::fs::write(
            self.snapshot.join("manifest.json"),
            serde_json::to_string_pretty(&manifest)? + "\n",
        )?;
        self.prune();
        Ok(())
    }

    /// Put back every protected file whose hash changed or that disappeared.
    /// Never fails loudly: this runs after the harness exits and a partial
    /// restore reported is better than a panic that reports nothing.
    pub fn restore(&self) -> Outcome {
        let mut outcome = Outcome {
            restored: Vec::new(),
            created: Vec::new(),
            verified: self.before.len(),
        };
        for path in &self.paths {
            match self.before.get(path) {
                None => {
                    if path.is_file() {
                        outcome.created.push(path.clone());
                    }
                }
                Some(was) => {
                    let unchanged =
                        path.is_file() && sha256(path).map(|now| &now == was).unwrap_or(false);
                    if unchanged {
                        continue;
                    }
                    let copy = self.copy_path(path);
                    let ok = path
                        .parent()
                        .map(|p| std::fs::create_dir_all(p).is_ok())
                        .unwrap_or(true)
                        && std::fs::copy(&copy, path).is_ok();
                    if ok {
                        outcome.restored.push(path.clone());
                    } else {
                        eprintln!(
                            "lmrun: FAILED to restore {} from {}",
                            path.display(),
                            copy.display()
                        );
                    }
                }
            }
        }
        outcome
    }

    fn copy_path(&self, path: &Path) -> PathBuf {
        let rel = path.strip_prefix(&self.root).unwrap_or(path);
        self.snapshot.join(rel)
    }

    fn prune(&self) {
        let Some(parent) = self.snapshot.parent() else {
            return;
        };
        let Ok(entries) = std::fs::read_dir(parent) else {
            return;
        };
        let mut dirs: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_dir())
            .collect();
        dirs.sort();
        if self.keep == 0 || dirs.len() <= self.keep {
            return;
        }
        for dir in &dirs[..dirs.len() - self.keep] {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

pub fn sha256(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("hashing {}", path.display()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_modified_protected_file_is_restored_and_a_new_one_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("home");
        std::fs::create_dir_all(root.join(".claude")).unwrap();
        let settings = root.join(".claude").join("settings.json");
        std::fs::write(&settings, "{\"marker\":\"original\"}").unwrap();

        let mut guard = Guard::new(
            &root,
            &dir.path().join("snaps"),
            "claude",
            &[".claude/settings.json", ".claude.json"],
            10,
        );
        guard.take().unwrap();

        std::fs::write(&settings, "TAMPERED").unwrap();
        std::fs::write(root.join(".claude.json"), "{}").unwrap();

        let outcome = guard.restore();
        assert_eq!(outcome.restored, vec![settings.clone()]);
        assert_eq!(outcome.created, vec![root.join(".claude.json")]);
        assert_eq!(outcome.verified, 1);
        assert_eq!(
            std::fs::read_to_string(&settings).unwrap(),
            "{\"marker\":\"original\"}"
        );
    }

    #[test]
    fn a_deleted_protected_file_comes_back() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("home");
        std::fs::create_dir_all(root.join(".codex")).unwrap();
        let cfg = root.join(".codex").join("config.toml");
        std::fs::write(&cfg, "model = \"x\"\n").unwrap();
        let mut guard = Guard::new(
            &root,
            &dir.path().join("snaps"),
            "codex",
            &[".codex/config.toml"],
            10,
        );
        guard.take().unwrap();
        std::fs::remove_file(&cfg).unwrap();
        let outcome = guard.restore();
        assert_eq!(outcome.restored.len(), 1);
        assert_eq!(std::fs::read_to_string(&cfg).unwrap(), "model = \"x\"\n");
    }

    #[test]
    fn old_snapshots_are_pruned_to_keep() {
        let dir = tempfile::tempdir().unwrap();
        let snaps = dir.path().join("snaps");
        for i in 0..5 {
            std::fs::create_dir_all(snaps.join(format!("2026010{i}-000000-x"))).unwrap();
        }
        let mut guard = Guard::new(dir.path(), &snaps, "x", &[], 3);
        guard.take().unwrap();
        let count = std::fs::read_dir(&snaps).unwrap().count();
        assert_eq!(count, 3);
        assert!(
            guard.snapshot.exists(),
            "the newest snapshot survives pruning"
        );
    }
}
