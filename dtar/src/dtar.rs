use std::{
    ffi::OsStr,
    fs::{self, File},
    io::{self, Write},
    os::unix::fs::MetadataExt,
    path::Path,
};

use decaf::*;
use flate2::Compression;

/// Writes a deterministically gzipped deterministic POSIX tar (ustar) archive of the passed directory to the writer
pub fn create_tar_gz<P: AsRef<Path>, W: Write>(
    directory_path: P,
    writer: &mut W,
) -> Result<(), io::Error> {
    create_tar(
        &directory_path,
        &mut flate2::GzBuilder::new()
            .extra("")
            .filename("")
            .operating_system(0)
            .mtime(0)
            .write(writer, Compression::fast()),
    )
}

/// Writes a deterministic POSIX tar (ustar) archive of the passed directory to the writer
pub fn create_tar<P: AsRef<Path>, W: Write>(
    directory_path: P,
    writer: &mut W,
) -> Result<(), io::Error> {
    let dir_path_as_path = Path::new(directory_path.as_ref());
    let top_level_directory = dir_path_as_path
        .file_name()
        .and_then(OsStr::to_str)
        .map(|s| {
            let mut dir = s.to_string();
            dir.push('/');
            dir
        })
        .unwrap_or_else(|| "./".to_string());

    let top_level_directory_perms = File::open(dir_path_as_path)?.metadata()?.mode();

    write_header(
        ArchivableListing {
            relative_path: top_level_directory.clone().into_boxed_str(),
            permissions: top_level_directory_perms,
            file_size: 0,
            literal_path: Default::default(),
        },
        writer,
    )?;

    for mut listing in create_archive_from_directory(&directory_path)?.listings {
        listing.relative_path = {
            let mut path_string = listing.relative_path.to_string();
            path_string.insert_str(0, top_level_directory.as_str());
            path_string.into_boxed_str()
        };
        write_header(listing, writer)?;
    }

    // write two blocks of zeros to mark the end of the tarball
    writer.write_all(&[0u8; 1024])?;

    Ok(())
}

fn write_header<W: Write>(listing: ArchivableListing, writer: &mut W) -> Result<(), io::Error> {
    let mut header_buffer = [0u8; 512];

    // get file content for listing if necessary
    let mut listing_content = Vec::with_capacity(listing.file_size as usize);

    if &listing.literal_path.to_str().unwrap() != &"" {
        listing_content = fs::read(&listing.literal_path)?;
    }

    // TODO: prefix paths with top level directory
    let path_bytes = listing.relative_path.as_bytes();
    let (name, prefix) = if path_bytes.len() <= 100 {
        (path_bytes, &[][..])
    } else {
        split_path(path_bytes)?
    };

    // name (100 bytes)
    header_buffer[..name.len()].copy_from_slice(name);

    // mode (8 bytes)
    write_octal(&mut header_buffer[100..108], listing.permissions as u64, 7);

    // uid (8 bytes) and gid (8 bytes) are null

    // file size (12 bytes)
    write_octal(
        &mut header_buffer[124..136],
        listing_content.len() as u64,
        11,
    );

    // mtime (12 bytes) is null

    // typeflag (1 byte)
    header_buffer[156] = if (listing.permissions & 0o040000) == 0o040000 {
        b'5' // directory
    } else {
        b'0' // regular file
    };

    // magic number (6 bytes)
    header_buffer[257..263].copy_from_slice(b"ustar\0");

    // version (2 bytes)
    header_buffer[263..265].copy_from_slice(b"00");

    // prefix (155 bytes)
    header_buffer[345..345 + prefix.len()].copy_from_slice(prefix);

    // calculate and write checksum
    let checksum = calculate_checksum(&header_buffer);
    write_octal(&mut header_buffer[148..156], checksum, 6);
    header_buffer[154] = b'\0';
    header_buffer[155] = b' ';

    writer.write_all(&header_buffer)?;
    writer.write_all(&listing_content)?;

    // pad file content to a multiple of 512 bytes
    let padding = (512 - (listing_content.len() % 512)) % 512;
    writer.write_all(&vec![0u8; padding])?;

    Ok(())
}

fn split_path(path: &[u8]) -> io::Result<(&[u8], &[u8])> {
    if path.len() > 255 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Path is too long: {} bytes", path.len()),
        ));
    }

    let split_index = path.len() - 100;
    let (prefix, _) = path.split_at(split_index);

    let adjusted_split = prefix
        .iter()
        .rposition(|&b| b == b'/')
        .map(|i| i + 1)
        .unwrap_or(0);

    Ok((&path[adjusted_split..], &path[..adjusted_split]))
}

fn write_octal(buffer: &mut [u8], value: u64, field_size: usize) {
    let octal = format!("{:0width$o}", value, width = field_size - 1);
    buffer[..octal.len()].copy_from_slice(octal.as_bytes());
    buffer[octal.len()] = 0;
}

fn calculate_checksum(header: &[u8; 512]) -> u64 {
    header.iter().enumerate().fold(0, |sum, (i, &byte)| {
        sum + if (148..156).contains(&i) {
            32
        } else {
            byte as u64
        }
    })
}
