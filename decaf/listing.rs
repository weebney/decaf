use std::cmp::Ordering;
use std::io::{self, Write};

use crate::archive;

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

pub trait Archivable {
    fn create_archive<W: Write>(self, writer: &mut W) -> Result<usize, io::Error>;
}

impl Archivable for Vec<Listing> {
    fn create_archive<W: Write>(self, writer: &mut W) -> Result<usize, io::Error> {
        archive(self, writer)
    }
}
