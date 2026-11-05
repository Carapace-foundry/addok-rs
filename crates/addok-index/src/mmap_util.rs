//! Dedicated, minimal memory mapping utility module for `addok-index`.
//!
//! Memory mapping is used for reading committed binary tables and database files
//! from disk into memory without requiring an intermediate userspace copy.

use memmap2::Mmap;
use std::fs::File;
use std::io::Result;
use std::path::Path;

/// Safely open a file read-only and create a memory map.
///
/// # Preconditions & Invariants
/// - The file at `path` is opened strictly in read-only mode (`File::open`).
/// - The file is expected to be an immutable database table (e.g. `.bin`, `.json`)
///   committed by the indexer.
/// - Callers must ensure the underlying file is not concurrently truncated or overwritten
///   in place by an external process.
pub fn mmap_read_only<P: AsRef<Path>>(path: P) -> Result<Mmap> {
    let file = File::open(path)?;
    mmap_file(&file)
}

/// Create a memory map from an already opened, read-only `File` reference.
///
/// # Safety
/// Undefined behavior in `memmap2::Mmap::map` can only occur if the underlying file
/// descriptor is mutated, truncated, or unlinked while mapped by another process.
/// In `addok-rs`:
/// 1. Files are opened read-only and managed by the single standalone process.
/// 2. Index files are write-once and atomic.
pub fn mmap_file(file: &File) -> Result<Mmap> {
    // SAFETY: The file descriptor is opened in read-only mode and is guaranteed
    // to be immutable for the duration of the index searcher lifecycle.
    unsafe { Mmap::map(file) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_mmap_read_only_success() {
        let mut tmp = NamedTempFile::new().expect("create temp file");
        tmp.write_all(b"hello addok mmap test")
            .expect("write bytes");
        tmp.flush().expect("flush");

        let mmap = mmap_read_only(tmp.path()).expect("mmap file");
        assert_eq!(&mmap[..], b"hello addok mmap test");
    }

    #[test]
    fn test_mmap_file_success() {
        let mut tmp = NamedTempFile::new().expect("create temp file");
        tmp.write_all(b"1234567890").expect("write bytes");
        tmp.flush().expect("flush");

        let file = File::open(tmp.path()).expect("open file");
        let mmap = mmap_file(&file).expect("mmap file");
        assert_eq!(&mmap[..], b"1234567890");
    }

    #[test]
    fn test_mmap_non_existent_file() {
        let res = mmap_read_only("non_existent_file_path_12345.bin");
        assert!(res.is_err());
    }
}
