use std::cmp::Ordering;
use std::fs;
use std::io;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use std::fs::File;
use std::io::BufWriter;

// internal representation
#[derive(Debug)]
pub struct Listing {
    // todo setup lifetimes for path so we can use &str
    path: Box<str>, // relative file or directory path
    permissions: u32,
    content_checksum: u32, // checksum of `content`
    content: Vec<u8>,      // binary content of file or empty if directory
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

fn create_listings(dir_path: &Path) -> Result<Vec<Listing>, io::Error> {
    let mut listings = Vec::new();
    let entries = fs::read_dir(dir_path)?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let metadata = entry.metadata()?;

        // directory handling
        if metadata.is_dir() {
            let sub_entries = fs::read_dir(&path)?;
            if sub_entries.count() == 0 {
                // directory is bare
                let path_str = path
                    .to_str()
                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Invalid path"))?;
                listings.push(Listing {
                    permissions: metadata.permissions().mode(),
                    content_checksum: crc32fast::hash(path_str.as_bytes()),
                    path: path_str.into(),
                    content: Default::default(),
                });
            } else {
                // recurse
                let mut sub_listings = create_listings(&path)?;
                listings.append(&mut sub_listings);
            }
            continue;
        }

        // file handling
        let binary = fs::read(&path)?;
        let perms = metadata.permissions().mode();
        let path_str = path.to_str().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "Failed to convert path to str")
        })?;

        listings.push(Listing {
            permissions: perms,
            path: path_str.into(),
            content_checksum: crc32fast::hash(binary.as_slice()),
            content: binary,
        });
    }

    listings.sort();
    Ok(listings)
}

pub fn create_archive<W: Write>(
    listings: Vec<Listing>,
    writer: &mut W,
) -> Result<(), std::io::Error> {
    let target_bundle_size = 1000 * 1000 * 10; //10mb bundle target size

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

        current_bundle_offset += listings[listing_idx].content.len();
        binary_bundles[bundle_idx].extend(listings[listing_idx].content.iter());

        let listing_path: &[u8] = listings[listing_idx].path.as_bytes();
        let listing_path_length: u16 = listing_path.len() as u16;
        let listing_permissions: u32 = listings[listing_idx].permissions;
        let listing_bundle_index: u64 = bundle_idx as u64;
        let listing_offset_in_bundle: u64 = current_bundle_offset as u64;
        let listing_checksum: u32 = listings[listing_idx].content_checksum;
        let listing_total_length: u64 = (listing_path.len() + 4 + 8 + 8 + 4 + 2) as u64;

        let mut listing_constructed: Vec<u8> = Default::default();
        listing_constructed.extend_from_slice(&listing_total_length.to_le_bytes());
        listing_constructed.extend_from_slice(&listing_path_length.to_le_bytes());
        listing_constructed.extend_from_slice(&listing_path);
        listing_constructed.extend_from_slice(&listing_bundle_index.to_le_bytes());
        listing_constructed.extend_from_slice(&listing_offset_in_bundle.to_le_bytes());
        listing_constructed.extend_from_slice(&listing_permissions.to_le_bytes());
        listing_constructed.extend_from_slice(&listing_checksum.to_le_bytes());

        binary_listings.push(listing_constructed);

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

    // write magic number
    archive_buffer.write_all(b"notdecaf")?;

    // write listing section
    let mut listing_section: Vec<u8> = Vec::new();
    for bl in binary_listings {
        listing_section.write_all(&bl)?;
    }
    // write listing section length
    archive_buffer.write_all(&(listing_section.len() as u64).to_le_bytes())?;
    // write listing section
    archive_buffer.write_all(&listing_section)?;

    // generate header info for bundles and compress bundles
    let mut bundle_header_buffer: Vec<u8> = Vec::with_capacity(binary_bundles.len());
    let mut compressed_bundles: Vec<Vec<u8>> = Vec::with_capacity(binary_bundles.len() * (8 + 4));
    let mut compressed_bundle_current_offset: u64 =
        (archive_buffer.len() + (binary_bundles.len() * (8 + 4))) as u64;
    for bundle in binary_bundles {
        let bundle_checksum = crc32fast::hash(&bundle);

        // setup the zstd encoder
        let mut zstd_enc = zstd::stream::Encoder::new(Vec::with_capacity(bundle.len()), 0)?;
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

    // write compressed bundles
    for compressed_bundle in &compressed_bundles {
        archive_buffer.write_all(&compressed_bundle)?;
    }

    // write checksum and archive content
    let archive_checksum: u32 = crc32fast::hash(archive_buffer.as_slice());
    writer.write_all(&archive_checksum.to_le_bytes())?;
    writer.write_all(&archive_buffer)?;

    Ok(())
}

pub fn main() -> Result<(), io::Error> {
    // create listings from dir
    let listings = create_listings(Path::new("/Users/weeb/Code/go"))?;

    let outfile = File::create("test.df")?;
    let mut writer = BufWriter::new(outfile);
    create_archive(listings, &mut writer)?;

    Ok(())
}
