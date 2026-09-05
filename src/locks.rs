//! Cooperating launches cannot share a generated home or restore the same files.
//! OS locks are released on process death. Never unlink the lock files: doing
//! so would let another process lock a different inode under the same name.
use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use std::os::{fd::AsRawFd, unix::fs::OpenOptionsExt};
use std::{fs::File, path::Path};

pub struct LaunchLocks {
    _files: Vec<File>,
}

impl LaunchLocks {
    pub fn acquire(root: &Path, snapshots: &Path, home: &Path, protected: &[&str]) -> Result<Self> {
        let directory = snapshots.join("locks");
        std::fs::create_dir_all(&directory)?;
        let mut paths: Vec<_> = protected.iter().map(|p| root.join(p)).collect();
        paths.push(home.to_path_buf());
        paths.sort();
        paths.dedup();
        let mut files = Vec::new();
        for path in paths {
            let name = format!(
                "{:x}.lock",
                Sha256::digest(path.as_os_str().as_encoded_bytes())
            );
            let file = File::options()
                .create(true)
                .truncate(false)
                .read(true)
                .write(true)
                .mode(0o600)
                .open(directory.join(name))
                .context("opening launch lock")?;
            // SAFETY: flock borrows this live descriptor; the file remains held
            // until launch, child shutdown and recovery have all completed.
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
                bail!(
                    "another launch protects {} (or its lock is unavailable): {}",
                    path.display(),
                    std::io::Error::last_os_error()
                );
            }
            files.push(file);
        }
        Ok(Self { _files: files })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overlapping_files_and_homes_are_locked_until_drop() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        let snapshots = root.join("snapshots");
        let first = LaunchLocks::acquire(root, &snapshots, &root.join("a"), &["config"]).unwrap();
        assert!(LaunchLocks::acquire(root, &snapshots, &root.join("b"), &["config"]).is_err());
        assert!(LaunchLocks::acquire(root, &snapshots, &root.join("a"), &["other"]).is_err());
        assert!(LaunchLocks::acquire(root, &snapshots, &root.join("b"), &["other"]).is_ok());
        drop(first);
        assert!(LaunchLocks::acquire(root, &snapshots, &root.join("a"), &["config"]).is_ok());
    }
}
