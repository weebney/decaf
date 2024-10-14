use std::cmp::Ordering;
use std::fs::{self, OpenOptions, Permissions};
use std::fs::{read_link, File};
use std::io::BufWriter;
use std::io::{self, Read, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::*;
use std::str::from_utf8;

use xxhash_rust::xxh3::xxh3_64 as xxh3;
use zstd::stream as zstd;

static MAGIC_NUMBER: u64 = u64::from_le_bytes(*b"iamdecaf");

// TODO: use .map_err() for all the ?s

// TODO: remove excessive buffering while writing archives; we can stitch data in whenever we want
// by using Trait std::io::Seek

// in general, we need to do way more pre-computation of buffer and file sizes etc etc

fn relative_path_from<P: AsRef<Path>, B: AsRef<Path>>(path: P, base: B) -> Option<PathBuf> {
    let path = path.as_ref();
    let base = base.as_ref();

    if path.is_absolute() != base.is_absolute() {
        if path.is_absolute() {
            Some(PathBuf::from(path))
        } else {
            None
        }
    } else {
        let mut ita = path.components();
        let mut itb = base.components();
        let mut comps: Vec<Component> = Vec::new();
        loop {
            match (ita.next(), itb.next()) {
                (None, None) => break,
                (Some(a), None) => {
                    comps.push(a);
                    comps.extend(ita.by_ref());
                    break;
                }
                (None, _) => comps.push(Component::ParentDir),
                (Some(a), Some(b)) if comps.is_empty() && a == b => (),
                (Some(a), Some(b)) if b == Component::CurDir => comps.push(a),
                (Some(_), Some(b)) if b == Component::ParentDir => return None,
                (Some(a), Some(_)) => {
                    comps.push(Component::ParentDir);
                    for _ in itb {
                        comps.push(Component::ParentDir);
                    }
                    comps.push(a);
                    comps.extend(ita.by_ref());
                    break;
                }
            }
        }
        Some(comps.iter().map(|c| c.as_os_str()).collect())
    }
}

#[derive(Debug)]
pub struct ArchivableListing {
    pub relative_path: Box<str>, // relative file or directory path
    pub permissions: u32,
    pub file_size: u64,
    pub literal_path: PathBuf,
}

impl Ord for ArchivableListing {
    fn cmp(&self, other: &Self) -> Ordering {
        // compare by content length
        self.file_size
            .cmp(&other.file_size)
            // compare by path length
            .then(self.relative_path.len().cmp(&other.relative_path.len()))
            // compare by permissions
            .then(self.permissions.cmp(&other.permissions))
    }
}

impl Eq for ArchivableListing {}

impl PartialEq for ArchivableListing {
    fn eq(&self, other: &Self) -> bool {
        self.file_size == other.file_size
            && self.relative_path.len() == other.relative_path.len()
            && self.permissions == other.permissions
    }
}

impl PartialOrd for ArchivableListing {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub struct ArchivableArchive {
    pub listings: Vec<ArchivableListing>,
}

impl ArchivableArchive {
    fn create_archive<W: Write>(&self, writer: &mut W) -> Result<usize, io::Error> {
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

            // get file content for listing if necessary
            let mut listing_content =
                Vec::with_capacity(self.listings[listing_idx].file_size as usize);
            let mut content_checksum = 0;

            if self.listings[listing_idx].literal_path.to_str().unwrap() != "" {
                listing_content = fs::read(&self.listings[listing_idx].literal_path)?;
                content_checksum = xxh3(&listing_content);
            }

            let listing_path: &[u8] = self.listings[listing_idx].relative_path.as_bytes();
            let listing_permissions: u32 = self.listings[listing_idx].permissions;
            let listing_bundle_index: u64 = bundle_idx as u64;
            let listing_offset_in_bundle: u64 = current_bundle_offset as u64;
            let listing_file_size: u64 = listing_content.len() as u64;
            let listing_checksum: u64 = content_checksum;
            let listing_total_length: u64 = (listing_path.len() + 44) as u64;

            let mut listing_constructed: Vec<u8> =
                Vec::with_capacity(listing_total_length as usize);
            listing_constructed.extend_from_slice(&listing_total_length.to_le_bytes());
            listing_constructed.extend_from_slice(&listing_bundle_index.to_le_bytes());
            listing_constructed.extend_from_slice(&listing_offset_in_bundle.to_le_bytes());
            listing_constructed.extend_from_slice(&listing_file_size.to_le_bytes());
            listing_constructed.extend_from_slice(&listing_permissions.to_le_bytes());
            listing_constructed.extend_from_slice(&listing_checksum.to_le_bytes());
            listing_constructed.extend_from_slice(listing_path);

            binary_listings.push(listing_constructed);

            current_bundle_offset += listing_content.len();
            binary_bundles[bundle_idx].append(&mut listing_content);

            listing_idx += 1;
            // check for listing exhaustion
            if listing_idx == self.listings.len() {
                break;
            }
        }

        // --------------------------------------------
        // generating the archive header data
        // --------------------------------------------

        let listing_section_total_length: usize = binary_listings.iter().map(|v| v.len()).sum();

        // generate header info for bundles and compress bundles
        let mut bundle_section: Vec<u8> = Vec::with_capacity(binary_bundles.len());
        let mut compressed_bundles: Vec<Vec<u8>> =
            Vec::with_capacity(binary_bundles.len() * (8 + 4));
        let mut compressed_bundle_current_offset: u64 =
            (listing_section_total_length + 40 + (binary_bundles.len() * 8 * 3)) as u64;

        for bundle in binary_bundles {
            let compressed_bundle_offset = compressed_bundle_current_offset;

            let bundle_checksum = xxh3(&bundle);

            // setup the zstd encoder
            let mut zstd_enc = zstd::Encoder::new(Vec::with_capacity(bundle.len()), 3)?;
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
        archive_buffer.write_all(&(listing_section_total_length as u64).to_le_bytes())?;

        // write listing count
        archive_buffer.write_all(&(self.listings.len() as u64).to_le_bytes())?;

        // write bundle count
        archive_buffer.write_all(&(compressed_bundles.len() as u64).to_le_bytes())?;

        // write listing block
        for bl in binary_listings.drain(..) {
            archive_buffer.write_all(&bl)?;
        }

        // write the bundle block
        archive_buffer.append(&mut bundle_section);

        // write compressed block
        for mut compressed_bundle in compressed_bundles.drain(..) {
            archive_buffer.append(&mut compressed_bundle);
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

    pub fn archive_to_file<P: AsRef<Path>>(
        &self,
        output_archive_path: P,
    ) -> Result<usize, io::Error> {
        let output_file = File::create(output_archive_path)?;
        let mut writer = BufWriter::new(output_file);
        self.create_archive(&mut writer)
    }

    pub fn archive_to_writer<W: Write>(&self, writer: &mut W) -> Result<usize, io::Error> {
        let mut writer = BufWriter::new(writer);
        self.create_archive(&mut writer)
    }
}

pub fn create_archive_from_directory<P: AsRef<Path>>(
    directory_path: P,
) -> Result<ArchivableArchive, io::Error> {
    create_archive_recursive(directory_path.as_ref(), directory_path.as_ref())
}

fn resolve_link<P: AsRef<Path>, B: AsRef<Path>>(
    path: P,
    parent_path: B,
) -> Result<bool, io::Error> {
    let resolved = read_link(path)?;
    if !resolved.starts_with(&parent_path) {
        return Ok(false);
    }
    if !resolved.metadata()?.is_symlink() {
        return Ok(true);
    }
    resolve_link(resolved, parent_path)
}

fn create_archive_recursive<P: AsRef<Path>, B: AsRef<Path>>(
    directory_path: P,
    parent_path: B,
) -> Result<ArchivableArchive, io::Error> {
    let mut local_listings = Vec::new();
    let entries = fs::read_dir(directory_path)?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let metadata = entry.metadata()?;

        if metadata.is_symlink() {
            if !resolve_link(&path, &parent_path)? {
                continue;
            } else {
                let can_path = path.canonicalize()?;
                let relative_path = relative_path_from(path, &parent_path).unwrap();
                let path_str = relative_path
                    .to_str()
                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Invalid path"))?;
                let perms = metadata.permissions().mode();
                local_listings.push(ArchivableListing {
                    permissions: perms,
                    relative_path: path_str.into(),
                    file_size: 0,
                    literal_path: can_path.clone(),
                });
                continue;
            }
        }

        // directory handling
        if metadata.is_dir() {
            let sub_entries = fs::read_dir(&path)?;
            if sub_entries.count() == 0 {
                // bare directory
                let relative_path = relative_path_from(path, &parent_path).unwrap();
                let path_str = relative_path
                    .to_str()
                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Invalid path"))?;
                local_listings.push(ArchivableListing {
                    permissions: metadata.permissions().mode(),
                    relative_path: path_str.into(),
                    file_size: 0,
                    literal_path: "".into(),
                });
            } else {
                // recurse
                let mut sub_listings = create_archive_recursive(&path, parent_path.as_ref())?;
                local_listings.append(&mut sub_listings.listings);
            }
            continue;
        }

        // file handling
        let perms = metadata.permissions().mode();
        let relative_path = relative_path_from(&path, parent_path.as_ref()).unwrap();
        let path_str = relative_path
            .to_str()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Invalid path"))?;

        let can_path = &path.canonicalize()?;

        let file_size = fs::metadata(can_path)?.size();

        local_listings.push(ArchivableListing {
            permissions: perms,
            relative_path: path_str.into(),
            file_size,
            literal_path: can_path.clone(),
        });
    }

    local_listings.sort();
    Ok(ArchivableArchive {
        listings: local_listings,
    })
}

#[derive(Debug)]
pub struct ExtractedListing {
    pub path: Box<str>, // relative file or directory path
    pub permissions: u32,
    pub content_checksum: u64, // checksum of `content`
    pub filesize: u64,
    pub bundle_idx: usize,
    pub bundle_offset: usize, // binary content of file or empty if directory
}

#[derive(Debug)]
pub struct ExtractedArchive {
    pub listings: Vec<ExtractedListing>,
    bundles: Vec<Vec<u8>>,
}

pub fn extract_from_file<P: AsRef<Path>>(archive_path: P) -> Result<ExtractedArchive, io::Error> {
    let mut archive_file = File::open(archive_path)?;
    extract_from_reader(&mut archive_file)
}

pub fn extract_from_reader<R: Read>(reader: &mut R) -> Result<ExtractedArchive, io::Error> {
    ExtractedArchive::from_reader(reader)
}

impl ExtractedArchive {
    pub fn from_reader<R: Read>(reader: &mut R) -> Result<ExtractedArchive, io::Error> {
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
        if u64::from_le_bytes(input_buffer[8..16].try_into().unwrap()) != xxh3(&input_buffer[16..])
        {
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
        let mut listings_vec: Vec<ExtractedListing> = Vec::with_capacity(listing_count as usize);

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
                &input_buffer
                    [current_offset + 44..current_offset + (listing_total_length as usize)],
            )
            .unwrap();

            current_offset += (listing_total_length) as usize;

            if listing_permissions & 0o040000 == 0o040000 {
                // bare directories
                listings_vec.push(ExtractedListing {
                    path: listing_path.into(),
                    permissions: listing_permissions,
                    content_checksum: 0,

                    bundle_idx: listing_bundle_index as usize,
                    bundle_offset: 0,
                    filesize: 0,
                });
                continue;
            }

            listings_vec.push(ExtractedListing {
                path: listing_path.into(),
                permissions: listing_permissions,
                content_checksum: listing_checksum,
                filesize: listing_file_size,
                bundle_idx: listing_bundle_index as usize,
                bundle_offset: listing_offset_in_uncompressed_bundle as usize,
            })
        }

        Ok(ExtractedArchive {
            listings: listings_vec,
            bundles: bundles_uncompressed,
        })
    }

    pub fn create_all_files<P: AsRef<Path>>(
        &self,
        output_directory_path: P,
    ) -> Result<usize, io::Error> {
        let mut sum: usize = 0;
        for listing in &self.listings {
            sum += self.create_file(listing, &output_directory_path)?;
        }
        Ok(sum)
    }

    pub fn create_file<P: AsRef<Path>>(
        &self,
        listing: &ExtractedListing,
        output_directory_path: P,
    ) -> Result<usize, io::Error> {
        let output_directory_path = Path::new(output_directory_path.as_ref());
        let mut listing_path = output_directory_path.to_path_buf();
        listing_path.push(listing.path.to_string());

        if listing.permissions & 0o040000 == 0o040000 {
            // bare directories
            fs::create_dir_all(listing_path).map_err(|e| {
                io::Error::new(e.kind(), format!("Failed to create bare directory: {}", e))
            })?;
            return Ok(0);
        }

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

        let mut listing_content = Vec::with_capacity(listing.filesize as usize);
        listing_content.write_all(
            &self.bundles[listing.bundle_idx]
                [listing.bundle_offset..listing.bundle_offset + listing.filesize as usize],
        )?;

        // verify listing content checksum
        let computed_checksum = xxh3(&listing_content);
        if computed_checksum != listing.content_checksum {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "invalid listing: could not verify file integrity for file {}, listing has {} but checksum was computed as {} (bundle {} with offset {}; size: {})",
                    listing.path, listing.content_checksum, computed_checksum, listing.bundle_idx, listing.bundle_offset, listing.filesize,
                ),
            ));
        }

        listing_file.write_all(&listing_content).map_err(|e| {
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
        Ok(listing.filesize as usize)
    }
}
