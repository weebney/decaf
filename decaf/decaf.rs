use std::cmp::Ordering;
use std::fs::File;
use std::fs::{self, OpenOptions, Permissions};
use std::io::BufWriter;
use std::io::{self, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::str::from_utf8;

use xxhash_rust::xxh3::xxh3_64 as xxh3;

mod relpath;
use relpath::*;

static MAGIC_NUMBER: u64 = u64::from_le_bytes(*b"notdecaf");

#[derive(Debug)]
pub struct Listing {
    pub path: Box<str>, // relative file or directory path
    pub permissions: u32,
    pub content_checksum: u64, // checksum of `content`
    pub content: Vec<u8>,      // binary content of file or empty if directory
}

impl Ord for Listing {
    fn cmp(&self, other: &Self) -> Ordering {
        // compare by content length
        self.content
            .len()
            .cmp(&other.content.len())
            // compare by path length
            .then(self.path.len().cmp(&other.path.len()))
            // compare by permissions
            .then(self.permissions.cmp(&other.permissions))
            // compare by flexible content
            .then(self.content.cmp(&other.content))
    }
}

impl PartialEq for Listing {
    fn eq(&self, other: &Self) -> bool {
        self.content.len() == other.content.len()
            && self.path.len() == other.path.len()
            && self.permissions == other.permissions
            && self.content == other.content
    }
}

impl PartialOrd for Listing {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Eq for Listing {}

pub trait DecafListing {
    fn create_archive<W: Write>(self, writer: &mut W) -> Result<usize, io::Error>;
    fn create_files<P: AsRef<Path>>(self, output_directory_path: P) -> Result<usize, io::Error>;
}

impl DecafListing for Vec<Listing> {
    fn create_archive<W: Write>(self, writer: &mut W) -> Result<usize, io::Error> {
        create_archive(self, writer)
    }

    fn create_files<P: AsRef<Path>>(self, output_directory_path: P) -> Result<usize, io::Error> {
        create_files(self, output_directory_path)
    }
}

pub fn archive_to_file<P: AsRef<Path>>(
    directory_path: P,
    output_archive_path: P,
) -> Result<usize, io::Error> {
    let listings = create_listings_from_directory(directory_path.as_ref())?;
    let output_file = File::create(output_archive_path)?;
    let mut writer = BufWriter::new(output_file);
    create_archive(listings, &mut writer)
}

pub fn archive_to_writer<P: AsRef<Path>, W: Write>(
    directory_path: P,
    writer: &mut W,
) -> Result<usize, io::Error> {
    let listings = create_listings_from_directory(directory_path.as_ref())?;
    let mut writer = BufWriter::new(writer);
    create_archive(listings, &mut writer)
}

pub fn create_listings_from_directory<P: AsRef<Path>>(
    directory_path: P,
) -> Result<Vec<Listing>, io::Error> {
    create_listings_recursive(directory_path.as_ref(), directory_path.as_ref())
}

fn create_listings_recursive<P: AsRef<Path>, B: AsRef<Path>>(
    directory_path: P,
    parent_path: B,
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
                let relative_path = relative_path_from(path, &parent_path).unwrap();
                let path_str = relative_path
                    .to_str()
                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Invalid path"))?;
                listings.push(Listing {
                    permissions: metadata.permissions().mode(),
                    content_checksum: 0,
                    path: path_str.into(),
                    content: Vec::with_capacity(0),
                });
            } else {
                // recurse
                let mut sub_listings = create_listings_recursive(&path, parent_path.as_ref())?;
                listings.append(&mut sub_listings);
            }
            continue;
        }

        // file handling
        let binary = fs::read(&path)?;
        let perms = metadata.permissions().mode();
        let relative_path = relative_path_from(&path, parent_path.as_ref()).unwrap();
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

fn create_archive<W: Write>(
    listings: Vec<Listing>,
    writer: &mut W,
) -> Result<usize, std::io::Error> {
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
    // generating the archive header data
    // --------------------------------------------

    // create listing section buffer
    let mut listing_section: Vec<u8> = Vec::new();
    for bl in binary_listings {
        listing_section.write_all(&bl)?;
    }

    // generate header info for bundles and compress bundles
    let mut bundle_section: Vec<u8> = Vec::with_capacity(binary_bundles.len());
    let mut compressed_bundles: Vec<Vec<u8>> = Vec::with_capacity(binary_bundles.len() * (8 + 4));
    let mut compressed_bundle_current_offset: u64 =
        (listing_section.len() + 40 + (binary_bundles.len() * 8 * 3)) as u64;

    for bundle in binary_bundles {
        let compressed_bundle_offset = compressed_bundle_current_offset;

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

        // size
        let compressed_bundle_size = compressed_bundle.len() as u64;

        // increment offset
        compressed_bundle_current_offset += compressed_bundle_size;

        bundle_section.write_all(&compressed_bundle_offset.to_le_bytes())?;
        bundle_section.write_all(&compressed_bundle_size.to_le_bytes())?;
        bundle_section.write_all(&bundle_checksum.to_le_bytes())?;
    }

    // --------------------------------------------
    // writing the archive buffer
    // --------------------------------------------

    let mut archive_buffer: Vec<u8> = Vec::new();

    // write listing block length
    archive_buffer.write_all(&(listing_section.len() as u64).to_le_bytes())?;

    // write listing count
    archive_buffer.write_all(&(listings.len() as u64).to_le_bytes())?;

    // write bundle count
    archive_buffer.write_all(&(compressed_bundles.len() as u64).to_le_bytes())?;

    // write listing block
    archive_buffer.write_all(&listing_section)?;

    // write the bundle block
    archive_buffer.write_all(&bundle_section)?;

    // write compressed block
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

pub fn unarchive_from_file<P: AsRef<Path>>(
    archive_path: P,
    output_directory_path: P,
) -> Result<usize, io::Error> {
    let mut archive_file = File::open(archive_path)?;
    unarchive_from_reader(&mut archive_file, output_directory_path)
}

pub fn unarchive_from_reader<R: Read, P: AsRef<Path>>(
    reader: &mut R,
    output_directory_path: P,
) -> Result<usize, io::Error> {
    let listings = unarchive_to_listings(reader)?;
    listings.create_files(output_directory_path)
}

fn create_files<P: AsRef<Path>>(
    listings: Vec<Listing>,
    output_directory_path: P,
) -> Result<usize, io::Error> {
    let output_directory_path = Path::new(output_directory_path.as_ref());

    for listing in listings {
        let mut listing_path = output_directory_path.to_path_buf();
        listing_path.push(listing.path.to_string());

        fs::create_dir_all(listing_path.parent().unwrap()).map_err(|e| {
            io::Error::new(
                e.kind(),
                format!("Failed to create ancestor directory: {}", e),
            )
        })?;

        File::create(listing_path.as_path()).map_err(|e| {
            io::Error::new(
                e.kind(),
                format!("Failed to create file {}: {}", listing_path.display(), e),
            )
        })?;

        let mut listing_file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&listing_path)
            .map_err(|e| {
                io::Error::new(
                    e.kind(),
                    format!(
                        "Failed to create/open file {} for writing: {}",
                        listing_path.display(),
                        e
                    ),
                )
            })?;

        listing_file.write_all(&listing.content).map_err(|e| {
            io::Error::new(
                e.kind(),
                format!(
                    "Failed to write content to file {}: {}",
                    listing_path.display(),
                    e
                ),
            )
        })?;

        listing_file
            .set_permissions(Permissions::from_mode(listing.permissions))
            .map_err(|e| {
                io::Error::new(
                    e.kind(),
                    format!(
                        "Failed to set permissions for file {}: {}",
                        listing_path.display(),
                        e
                    ),
                )
            })?;
    }

    Ok(0)
}

pub fn unarchive_to_listings<R: Read>(reader: &mut R) -> Result<Vec<Listing>, io::Error> {
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
    let bundle_count = u64::from_le_bytes(input_buffer[32..40].try_into().unwrap());

    let mut bundles_uncompressed: Vec<Vec<u8>> = Vec::new();
    let mut current_offset: usize = listing_block_length as usize + 40;
    for i in 0..bundle_count {
        let compressed_bundle_offset = u64::from_le_bytes(
            input_buffer[current_offset..current_offset + 8]
                .try_into()
                .unwrap(),
        );

        let compressed_bundle_size = u64::from_le_bytes(
            input_buffer[current_offset + 8..current_offset + 16]
                .try_into()
                .unwrap(),
        );

        let uncompressed_bundle_checksum = u64::from_le_bytes(
            input_buffer[current_offset + 16..current_offset + 24]
                .try_into()
                .unwrap(),
        );

        current_offset += 8 * 3;

        let mut decompression_buffer = Vec::with_capacity(compressed_bundle_size as usize);
        decompression_buffer.write_all(
            &input_buffer[compressed_bundle_offset as usize
                ..compressed_bundle_offset as usize + compressed_bundle_size as usize],
        )?;

        let mut zstd_dec = zstd::Decoder::new(decompression_buffer.as_slice())?;
        let mut uncompressed_bundle_content = Vec::new();
        zstd_dec.read_to_end(&mut uncompressed_bundle_content)?;

        // verify bundle checksum
        if xxh3(&uncompressed_bundle_content) != uncompressed_bundle_checksum {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "invalid archive: could not verify bundle integrity for bundle {}",
                    i
                ),
            ));
        }

        bundles_uncompressed.push(uncompressed_bundle_content);
    }

    // create listings vector
    let mut listings: Vec<Listing> = Vec::with_capacity(listing_count as usize);

    current_offset = 40;
    for _ in 0..listing_count {
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

        current_offset += (listing_total_length) as usize;

        if listing_permissions & 0o040000 == 0o040000 {
            // bare directories
            listings.push(Listing {
                path: listing_path.into(),
                permissions: listing_permissions,
                content_checksum: 0,
                content: Vec::with_capacity(0),
            });
            continue;
        }

        let mut listing_content = Vec::with_capacity(listing_file_size as usize);

        listing_content.write_all(
            &bundles_uncompressed[listing_bundle_index as usize]
                [listing_offset_in_uncompressed_bundle as usize
                    ..listing_offset_in_uncompressed_bundle as usize + listing_file_size as usize],
        )?;

        // verify listing content checksum
        let computed_checksum = xxh3(&listing_content);
        if computed_checksum != listing_checksum {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "invalid listing: could not verify file integrity for file {}, listing has {} but checksum was computed as {} (bundle {} with offset {}; size: {})",
                    listing_path, listing_checksum, computed_checksum, listing_bundle_index, listing_offset_in_uncompressed_bundle, listing_file_size,
                ),
            ));
        }

        listings.push(Listing {
            path: listing_path.into(),
            permissions: listing_permissions,
            content_checksum: listing_checksum,
            content: listing_content,
        })
    }

    Ok(listings)
}
