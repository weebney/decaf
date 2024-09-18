# Deterministic Compressed Archive Format (DeCAF)

The Deterministic Compressed Archive Format (DeCAF) is an archive file format that offers a large number of benefits over existing archive formats:

- Order-of-magnitude faster; 10x faster archiving and unarchiving vs `tar -czvf`
- Fully deterministic, platform agnostic archives; `one set of files == one archive`, regardless of platform, operating system, etc.
- Random access; if you only need one file, you only have to decompress a small section of the archive
- Inherent integrity; archive and file integrity is inherently verified at every step of the unarchiving process
- Built on modern standards, including `zstd` and `xxh3`

This repository contains all of the official implementations, tools, and documentation related to DeCAF:

- `decaf/`; the DeCAF reference implementation in Rust
- `decaf-capi/`; official C bindings for the `decaf` Rust crate
- `libdecaf/`;  an official DeCAF implementation in C99
- `decaf-cli/`; the `dar` command line utility for manipulating DeCAF archives
- `doc/`; the official specification for DeCAF and its supporting documentation
- `dtar/`; a Rust library for very fast, deterministic POSIX tar archiving used in `dar`

## Rationale

Why do we need DeCAF?

## Limitations

- DeCAF ignores symlinks entirely; symbolic links within an archive are an antipattern.
