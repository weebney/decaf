use std::fs;
use std::fs::File;
use std::io;
use std::io::BufWriter;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::str::from_utf8;

use xxhash_rust::xxh3::xxh3_64 as xxh3;

pub mod listing;
use listing::*;

mod relpath;
use relpath::*;

static MAGIC_NUMBER: u64 = u64::from_le_bytes(*b"iamdecaf");

pub fn archive_to_file<'a>(
    directory_path: &'a Path,
    output_archive_path: &'a Path,
) -> Result<usize, io::Error> {
    let listings = create_listings_from_directory(&directory_path)?;
    let output_file = File::create(output_archive_path)?;
    let mut writer = BufWriter::new(output_file);
    archive(listings, &mut writer)
}

pub fn archive_to_writer<'a, W: Write>(
    directory_path: &'a Path,
    writer: &mut W,
) -> Result<usize, io::Error> {
    let listings = create_listings_from_directory(&directory_path)?;
    let mut writer = BufWriter::new(writer);
    archive(listings, &mut writer)
}

pub fn create_listings_from_directory(directory_path: &Path) -> Result<Vec<Listing>, io::Error> {
    create_listings_recursive(directory_path, directory_path)
}

fn create_listings_recursive(
    directory_path: &Path,
    parent_path: &Path,
) -> Result<Vec<Listing>, io::Error> {
    let mut listings = Vec::new();
    let entries = fs::read_dir(directory_path)?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let metadata = entry.metadata()?;

        if metadata.is_symlink() {
            continue;
        }

        // directory handling
        if metadata.is_dir() {
            let sub_entries = fs::read_dir(&path)?;
            if sub_entries.count() == 0 {
                // directory is bare
                let relative_path = relative_path_from(path, parent_path).unwrap();
                let path_str = relative_path
                    .to_str()
                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Invalid path"))?;
                listings.push(Listing {
                    permissions: metadata.permissions().mode(),
                    content_checksum: 0,
                    path: path_str.into(),
                    content: Default::default(),
                });
            } else {
                // recurse
                let mut sub_listings = create_listings_recursive(&path, parent_path)?;
                listings.append(&mut sub_listings);
            }
            continue;
        }

        // file handling
        let binary = fs::read(&path)?;
        let perms = metadata.permissions().mode();
        let relative_path = relative_path_from(path, parent_path).unwrap();
        let path_str = relative_path
            .to_str()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Invalid path"))?;

        listings.push(Listing {
            permissions: perms,
            path: path_str.into(),
            content_checksum: xxh3(binary.as_slice()),
            content: binary,
        });
    }

    listings.sort();
    Ok(listings)
}

fn archive<W: Write>(listings: Vec<Listing>, writer: &mut W) -> Result<usize, std::io::Error> {
    let target_bundle_size = 1000 * 1000 * 10; // 10mb target bundle size

    let mut binary_listings: Vec<Vec<u8>> = Vec::new();
    let mut binary_bundles: Vec<Vec<u8>> = Vec::new();

    let mut listing_idx = 0;
    binary_bundles.push(Vec::new());
    let mut bundle_idx = 0;
    let mut current_bundle_offset = 0;
    loop {
        if binary_bundles[bundle_idx].len() > target_bundle_size {
            binary_bundles.push(Vec::new());
            current_bundle_offset = 0;
            bundle_idx += 1;
        }

        let listing_path: &[u8] = listings[listing_idx].path.as_bytes();
        let listing_permissions: u32 = listings[listing_idx].permissions;
        let listing_bundle_index: u64 = bundle_idx as u64;
        let listing_offset_in_bundle: u64 = current_bundle_offset as u64;
        let listing_file_size: u64 = listings[listing_idx].content.len() as u64;
        let listing_checksum: u64 = listings[listing_idx].content_checksum;
        let listing_total_length: u64 = (listing_path.len() + 44) as u64;

        let mut listing_constructed: Vec<u8> = Vec::with_capacity(listing_total_length as usize);
        listing_constructed.extend_from_slice(&listing_total_length.to_le_bytes());
        listing_constructed.extend_from_slice(&listing_bundle_index.to_le_bytes());
        listing_constructed.extend_from_slice(&listing_offset_in_bundle.to_le_bytes());
        listing_constructed.extend_from_slice(&listing_file_size.to_le_bytes());
        listing_constructed.extend_from_slice(&listing_permissions.to_le_bytes());
        listing_constructed.extend_from_slice(&listing_checksum.to_le_bytes());
        listing_constructed.extend_from_slice(&listing_path);

        binary_listings.push(listing_constructed);

        current_bundle_offset += listings[listing_idx].content.len();
        binary_bundles[bundle_idx].extend(listings[listing_idx].content.iter());

        listing_idx += 1;
        // check for listing exhaustion
        if listing_idx == listings.len() {
            break;
        }
    }

    // --------------------------------------------
    // writing the archive header
    // --------------------------------------------

    let mut archive_buffer: Vec<u8> = Vec::new();

    // write listing section
    let mut listing_section: Vec<u8> = Vec::new();
    for bl in binary_listings {
        listing_section.write_all(&bl)?;
    }

    // write listing section length
    archive_buffer.write_all(&(listing_section.len() as u64).to_le_bytes())?;

    // write listing count
    archive_buffer.write_all(&(listings.len() as u64).to_le_bytes())?;

    // write listing section
    archive_buffer.write_all(&listing_section)?;

    // generate header info for bundles and compress bundles
    let mut bundle_header_buffer: Vec<u8> = Vec::with_capacity(binary_bundles.len());
    let mut compressed_bundles: Vec<Vec<u8>> = Vec::with_capacity(binary_bundles.len() * (8 + 4));
    let mut compressed_bundle_current_offset: u64 =
        (archive_buffer.len() + (binary_bundles.len() * (8 + 4))) as u64;

    for bundle in binary_bundles {
        let bundle_checksum = xxh3(&bundle);

        // setup the zstd encoder
        let mut zstd_enc = zstd::stream::Encoder::new(Vec::with_capacity(bundle.len()), 3)?;
        zstd_enc.set_pledged_src_size(Some(bundle.len() as u64))?;
        zstd_enc.include_checksum(false)?;
        zstd_enc.include_contentsize(false)?;

        // compress the bundle
        zstd_enc.write_all(&bundle)?;
        let compressed_bundle = zstd_enc.finish()?;
        compressed_bundles.push(compressed_bundle.clone());

        // compute offset
        let compressed_bundle_offset = compressed_bundle_current_offset;

        // increment offset
        compressed_bundle_current_offset += compressed_bundle.len() as u64;

        bundle_header_buffer.write_all(&compressed_bundle_offset.to_le_bytes())?;
        bundle_header_buffer.write_all(&bundle_checksum.to_le_bytes())?;
    }

    // write the bundle header to the archive buffer
    archive_buffer.write_all(&bundle_header_buffer)?;

    // write compressed bundles to the archive buffer
    for compressed_bundle in &compressed_bundles {
        archive_buffer.write_all(&compressed_bundle)?;
    }

    // --------------------------------------------
    // writing the actual archive
    // --------------------------------------------

    // write magic number
    writer.write_all(&MAGIC_NUMBER.to_le_bytes())?;

    // write checksum
    let archive_checksum: u64 = xxh3(archive_buffer.as_slice());
    writer.write_all(&archive_checksum.to_le_bytes())?;

    // write archive
    writer.write_all(&archive_buffer)?;

    Ok(16 + archive_buffer.len()) // 8 bytes for the magic number, 8 bytes for the checksum
}

pub fn unarchive_from_file<'a>(
    archive_path: &'a Path,
    output_directory_path: &'a Path,
) -> Result<usize, io::Error> {
    let mut archive_file = File::open(archive_path)?;
    unarchive(&mut archive_file, output_directory_path)
}

fn unarchive<R: Read>(reader: &mut R, output_directory_path: &Path) -> Result<usize, io::Error> {
    let mut input_buffer: Vec<u8> = Vec::new();
    reader.read_to_end(&mut input_buffer)?;

    if input_buffer.len() < 64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "invalid archive: archive too small with size {} bytes",
                input_buffer.len()
            ),
        ));
    };

    // verify magic number
    if input_buffer[0..8] != MAGIC_NUMBER.to_le_bytes() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid archive: does not contain magic number",
        ));
    }

    // verify archive checksum
    if u64::from_le_bytes(input_buffer[8..16].try_into().unwrap()) != xxh3(&input_buffer[16..]) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid archive: could not verify archive integrity",
        ));
    }

    let listing_block_length = u64::from_le_bytes(input_buffer[16..24].try_into().unwrap());
    let listing_count = u64::from_le_bytes(input_buffer[24..32].try_into().unwrap());

    // create listings vector
    let listings: Vec<Listing> = Vec::with_capacity(listing_count as usize);

    let mut current_offset: usize = 32;
    for i in 0..listing_count {
        let listing_total_length = u64::from_le_bytes(
            input_buffer[current_offset..current_offset + 8]
                .try_into()
                .unwrap(),
        );
        let listing_bundle_index = u64::from_le_bytes(
            input_buffer[current_offset + 8..current_offset + 16]
                .try_into()
                .unwrap(),
        );
        let listing_offset_in_uncompressed_bundle = u64::from_le_bytes(
            input_buffer[current_offset + 16..current_offset + 24]
                .try_into()
                .unwrap(),
        );
        let listing_file_size = u64::from_le_bytes(
            input_buffer[current_offset + 24..current_offset + 32]
                .try_into()
                .unwrap(),
        );
        let listing_permissions = u32::from_le_bytes(
            input_buffer[current_offset + 32..current_offset + 36]
                .try_into()
                .unwrap(),
        );
        let listing_checksum = u64::from_le_bytes(
            input_buffer[current_offset + 36..current_offset + 44]
                .try_into()
                .unwrap(),
        );
        let listing_path = from_utf8(
            &input_buffer[current_offset + 44..current_offset + (listing_total_length as usize)],
        )
        .unwrap();
        println!(
            "-------- {}\nlen: {}\nbidx: {}\nofst: {}\nsize: {}\nperms: {:o}\ncksm: {:x}\npath: {:?}",
            i,
            listing_total_length,
            listing_bundle_index,
            listing_offset_in_uncompressed_bundle,
            listing_file_size,
            listing_permissions,
            listing_checksum,
            listing_path
        );
        current_offset += (listing_total_length) as usize;
    }

    Ok(0)
}
