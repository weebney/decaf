use std::fs::{self};
use std::path::*;
use std::time::Instant;
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

    let now = Instant::now();
    let listings_result = create_archive_from_directory(base_path);
    assert!(listings_result.is_ok());
    println!("gather  {}", now.elapsed().as_millis());

    let nowa = Instant::now();
    let archive_result = listings_result.unwrap().archive_to_file(&archive_path);
    assert!(archive_result.is_ok());
    println!("archive {}", nowa.elapsed().as_millis());
    println!("------------ {}", now.elapsed().as_millis());

    let now3 = Instant::now();
    let extract_result = extract_from_file(&archive_path);
    assert!(extract_result.is_ok());
    println!("extract {}", now3.elapsed().as_millis());

    let now4 = Instant::now();
    let create_files_result = extract_result.unwrap().create_all_files(&extract_path);
    assert!(create_files_result.is_ok());
    println!("place   {}", now4.elapsed().as_millis());
    println!("------------ {}", now3.elapsed().as_millis());
    println!("all     {}", now.elapsed().as_millis());

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
