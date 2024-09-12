use dtar::*;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::process::{Command, ExitStatus};

#[test]
fn system_tar_diff() {
    let dtar_outpath = "/tmp/test_dtar.tar.gz";
    let system_tar_outpath = "/tmp/test_system_tar.tar.gz";
    let inpath = "./";

    let mut dtar_outfile = File::create(dtar_outpath).unwrap();
    create_tar(inpath, &mut dtar_outfile).unwrap();
    Command::new("tar")
        .args(["-cf", system_tar_outpath, inpath])
        .output()
        .unwrap();

    let dtar_extraction_dir = "/tmp/dtar_diffdir";
    let system_tar_extraction_dir = "/tmp/sys_tar_diffdir";
    fs::create_dir(dtar_extraction_dir).unwrap_or(());
    fs::create_dir(system_tar_extraction_dir).unwrap_or(());
    Command::new("tar")
        .args(["-zxf", dtar_outpath, "-C", dtar_extraction_dir])
        .output()
        .unwrap();
    Command::new("tar")
        .args(["-zxf", system_tar_outpath, "-C", system_tar_extraction_dir])
        .output()
        .unwrap();

    // diff dirs
    assert_eq!(
        Command::new("diff")
            .args(["-r", dtar_extraction_dir, system_tar_extraction_dir])
            .output()
            .unwrap()
            .status,
        ExitStatus::default(),
    );

    fs::remove_dir_all(dtar_extraction_dir).unwrap();
    fs::remove_dir_all(system_tar_extraction_dir).unwrap();
    fs::remove_file(dtar_outpath).unwrap();
    fs::remove_file(system_tar_outpath).unwrap();
}

#[test]
fn gzip_determinism() {
    let file_a_path = "/tmp/test_determinism_a.tar.gz";
    let file_b_path = "/tmp/test_determinism_b.tar.gz";

    {
        let mut outfilea = File::create(file_a_path).unwrap();
        let mut outfileb = File::create(file_b_path).unwrap();
        create_tar_gz(Path::new("../decaf"), &mut outfilea).unwrap();
        create_tar_gz(Path::new("../decaf"), &mut outfileb).unwrap();
    }

    let mut filea = File::open(file_a_path).unwrap();
    let mut fileb = File::open(file_b_path).unwrap();

    let mut a_buf = Vec::new();
    let mut b_buf = Vec::new();
    filea.read_to_end(&mut a_buf).unwrap();
    fileb.read_to_end(&mut b_buf).unwrap();

    assert_eq!(a_buf, b_buf);

    std::fs::remove_file(file_a_path).unwrap();
    std::fs::remove_file(file_b_path).unwrap();
}
