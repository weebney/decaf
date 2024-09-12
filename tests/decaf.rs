use std::fs::{self};
use std::path::*;
use tempfile::TempDir;

use decaf::*;

#[test]
fn archive_and_unarchive() {
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path();
    let archive_path = base_path.join("archive.df");
    let extract_path = base_path.join("extracted");

    fs::create_dir(base_path.join("subdir")).unwrap();
    fs::write(base_path.join("file1.txt"), "Hello, world!").unwrap();
    fs::write(
        base_path.join("subdir/file2.txt"),
        "Slightly larger test content",
    )
    .unwrap();

    let archive_result = archive_to_file(Path::new("../go/"), &archive_path);
    assert!(archive_result.is_ok());

    let unarchive_result = unarchive_from_file(&archive_path, &extract_path);
    println!("{:?}", unarchive_result);
    assert!(unarchive_result.is_ok());

    // assert!(extract_path.join("file1.txt").exists());
    // assert!(extract_path.join("subdir/file2.txt").exists());
    // assert_eq!(
    //     fs::read_to_string(extract_path.join("file1.txt")).unwrap(),
    //     "Hello, world!"
    // );
    // assert_eq!(
    //     fs::read_to_string(extract_path.join("subdir/file2.txt")).unwrap(),
    //    "Slightly larger test content",
    //);
}
