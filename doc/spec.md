# The Deterministic Compressed Archive Format (DeCAF)

> [!CAUTION]
> **THIS IS AN UNFINISHED DRAFT SPECIFICATION!**
> Changes to this document may not be reflected in the source code (and vice versa)!

## Abstract

The reproducibility of build-time artifacts has become an important factor in the development of a more secure and robust software supply-chain.

## Introduction

The Deterministic Compressed Archive Format (DeCAF) operates on a relatively simple principle; one set of files, regardless of platform, can only produce one valid DeCAF archive. Conversely, one DeCAF archive can only represent and produce one set of files. The format specifies a way to represent files and directories in stream-oriented, abstract flat filesystems (referred to as _archives_) that encode the following information about files (referred to _listings_):

- the path of the listings
- the mode of the listings
- the content of the listings

This is all the information a DeCAF archive stores about listings and therefore all that needs to be known about a set of files to produce the listings in a DeCAF archives. Any platform which natively supports files that encode this information (or in which this information can be reproducibly derived) can produce and consume DeCAF archives in a native manner. However, because DeCAF archives are effectively their own filesystem, they can also be composed or consumed on virtually any system, even those with no filesystem or standard library, albeit with a compatibility layer.

DeCAF effectively normalizes files and directories when an archive is created. This means that, while one set of files may produce a different set of files when they are archived and unarchived, that new set of files will _always_ produce an identical set of files when unarchived.

The layout of a DeCAF archive is as follows:

| Name | Contains |
| ---: | ---: |
| Archive Header | General information used to parse the rest of the archive |
| Listing Header | Listings, which encode the metadata of the files in the archive |
| Bundle Header | Information for extracting individual bundles in the compressed section |
| Compressed Section | Compressed bundles, which contain the actual content of files |

## Conventions and Definitions

- byte: a single octet or uint8
- stream: an ordered sequence of any number of bytes, back-to-back with no padding. All streams are encoded in "little-endian" byte order (that is, least significant byte first).
- string: an indefinite-length stream of uint8s, representing a UTF-8 encoded string with no null terminator. The largest possible string that can be represented is one with a length of 65,535.

### xxHash3 & Checksums

For data integrity and checksum operations, DeCAF exclusively uses the XXH3-64 variant of the xxHash fast digest algorithm. It is defined in the [xxHash3 specification document](https://github.com/Cyan4973/xxHash/blob/dev/doc/xxhash_spec.md) by Yann Collet.

## Overview

### Scope of an Archive

The _scope_ of an archive is defined by the most significant shared ancestor of every file in an archive and determines the "root" (called the _apex_) of the archive's internal filesystem.

As an example, here is a representation of the filetree of a fake `/home/` directory.

```
/home/
├── passwords.txt
├── random.dat
├── photos/
│   ├── selfie.jpg
│   ├── drawing.png
│   └── vacation/
│       └── goats.jpg
└── documents/
```

If we were to make a DeCAF archive of the `/home/` directory, `/home/` would become the apex of the archive and the scope would be every file that descends from its corresponding directory on the filesystem, which would be the full tree here. The internal flat filesystem of the resulting archive would look something like this:

```
passwords.txt
random.dat
photos/selfie.jpg
photos/drawing.png
photos/vacation/goats.jpg
documents/
```

If we were to make an archive from the `/home/photos/` directory, though, the scope would be reduced such that the internal filesystem of the archive would be something like:

```
selfie.jpg
drawing.png
vacation/goats.jpg
```

## Listings & Listing Header

Listings provide all the information about a file in an archive. They are the unit that comprises the listing section of the header.

Listings are composed in the following manner:

| Description | Type |
| ---: | ---: |
| the index of the bundle which contains the content of this listing | uint64 |
| the offset (in # of bytes) within the bundle where the content of this listing begins | uint64 |
| the size (in # of bytes) of the content | uint64 |
| the checksum of the listing content | uint64 |
| the mode of this listing | uint8 |
| the number of bytes in the path of this listing | uint16 |
| the path of the listing | string |

### Mode

Modes encode the type and permissions for a listing. They are a single byte, determined in the following manner:

- normal            (0, 00000000)
- executable        (1, 00000001)
- link              (2, 00000010)
- bare directory    (3, 00000011)

DeCAF archives have no conception of users, groups, or permissions. All normal, executable, and link files are assumed to be readable and writeable. Because archives are a flat filesystem, directories that contain files exist implicity and are not given listings. Only bare directories, which have no files, are written explicitly to the archive. The link mode provides a platform agnostic way to represent files that point to another file, like symbolic links.

Links which point outside the scope of the archive can not be represented, so they can not be archived.

For example, during extraction in the reference implementation of DeCAF (for UNIX-like systems), normal files are always given a mode on the filesystem of `100644`, executable files are given `100755`, and links are given `120000`, mimicing the functionality of `git`[^1]. During archiving, only the permissions for the owner are read; if the file is not readable or writable by the owner, it's skipped.

### Path

a relative path that is lexically equivalent to targpath when joined to basepath with an intervening separator.


### Ordering of Listings

Listings are ordered in the following manner:

1. Filesize, from smallest to largest

If there are conflicts (i.e. two listings have the same filesize), then the two listings are ordered by:

2. Path length, from shortest to longest

If there are still conflicts (i.e. two listings have the same filesize and their paths are the same length), then the two listings are ordered by:

3. Path, lexicographically

### Assignment of Bundle Information to Listings

Bundle indexes can be assigned to listings with a simple iterative algorithm:

```
function AssignBundleIndexes(ListingsArray) {
  const targetBundleSize = 10*1024^2
  var currentBundleIndex = 0
  var currentBundleSize = 0
  for listing in ListingsArray {
    if currentBundleSize < targetBundleSize {
      currentBundleIndex += 1
      currentBundleSize = 0
    }
    currentBundleSize += listing.ContentSize
    listing.BundleIndex = currentBundleIndex
  }
}
```

Offsets within the uncompressed bundle can be assigned to listings with a simple iterative algorithm:

```
function AssignBundleOffset(ListingsArray) {
  var currentBundleIndex = 0
  var currentBundleSize = 0
  for listing in ListingsArray {
    if listing.BundleIndex > currentBundleIndex {
      currentBundleIndex += 1
      currentBundleSize = 0
    }
    listing.BundleOffset = currentBundleSize
    currentBundleSize += listing.ContentSize
  }
}
```

## Bundle Header



## References

[^1]: https://github.com/git/git/blob/ef8ce8f3d4344fd3af049c17eeba5cd20d98b69f/Documentation/git-fast-import.txt#L616
