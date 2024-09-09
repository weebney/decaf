use std::fs::{self};
use std::path::*;
use tempfile::TempDir;

use decaf::*;

#[test]
fn test_archive_and_unarchive() {
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path();
    let archive_path = base_path.join("archive.df");
    let extract_path = base_path.join("extracted");

    // Create sample files
    fs::create_dir(base_path.join("subdir")).unwrap();
    fs::write(base_path.join("file1.txt"), b"Hello, world!").unwrap();
    fs::write(
        base_path.join("subdir/file2.txt"),
        b"Slightly larger test content",
    )
    .unwrap();

    // Archive
    let archive_result = archive_to_file(Path::new("../go"), &archive_path);
    assert!(archive_result.is_ok());

    // Unarchive (assuming you have an unarchive function)
    let unarchive_result = unarchive_from_file(&archive_path, &extract_path);
    assert!(unarchive_result.is_ok());

    // Verify extracted contents
    // assert!(extract_path.join("file1.txt").exists());
    // assert!(extract_path.join("subdir/file2.txt").exists());
    // assert_eq!(fs::read_to_string(extract_path.join("file1.txt")).unwrap(), "Hello, world!");
    // assert_eq!(fs::read_to_string(extract_path.join("subdir/file2.txt")).unwrap(), "Test content");
}
