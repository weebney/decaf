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

The layout of a DeCAF archive is as follows:

## Archives

Archives are comprised of the _header_, which defines the layout of the filesystem and provides information required for extraction, and the _data section_, which contains the compressed content of the files defined in the header.

## Bundles

Bundles contain the uncompressed content of files.

## Listings

Listings are composed in the following manner:

- total length of this listing
- the index of the bundle which contains the content of this listing
- the offset (in # of bytes) within the bundle where the content of this listing begins
- the size (in # of bytes) of the content
- the mode of this listing
- the XXH3 checksum of the content
- the path of the listing

### Mode

Modes encode the type and permissions for a listing. They are an 8-bit stream defined in the following manner.

- normal            (0, 00000000)
- executable        (1, 00000001)
- link              (2, 00000010)
- bare directory    (3, 00000011)

DeCAF has no conception of users, groups, or permissions. All normal, executable, and link files are assumed to be readable and writeable. Because archives are a flat filesystem, directories that contain files exist implicity and are not given listings. Only bare directories, which have no files, are written explicitly to the archive. The link mode provides a platform agnostic way to represent files that point to another file.

For example, during extraction in the reference implementation of DeCAF (for UNIX systems) normal files are always given a mode on the filesystem of `100644`, executable files are given `100755`, and links are given `120000`, mimicing the functionality of `git`[^1]. During archiving, only the permissions for the owner are read; if the file is not readable or writable by the owner, it's skipped.

## References

[^1]: https://github.com/git/git/blob/ef8ce8f3d4344fd3af049c17eeba5cd20d98b69f/Documentation/git-fast-import.txt#L616
