# Deterministic Compressed Archive Format (DeCAF)

> [!CAUTION]
> The DeCAF specification is not finished and will likely change.
> This repository is currently available in an extremely early, pre-release form.
> The first version will release with the completion of the specification.

The Deterministic Compressed Archive Format (DeCAF) is an archive file format that offers a large number of benefits over existing archive formats:

- Order-of-magnitude faster; 10x faster archiving and unarchiving vs `tar -czvf`
- Fully deterministic, platform agnostic archives; `one set of files == one archive`, regardless of platform, operating system, etc.
- Random access; if you only need one file, you only have to decompress a small section of the archive
- Inherent integrity; archive and file integrity is inherently verified at every step of the unarchiving process
- Built on modern standards, including `zstd` and `xxh3`

This repository contains all of the official implementations, tools, and documentation related to DeCAF:

- [`decaf/`](./decaf/); the official DeCAF implementation in Rust
- [`decaf-cli/`](./decaf-cli/); the `dar` command line utility for manipulating DeCAF archives
- [`dtar/`](./dtar/); a Rust library for very fast, deterministic POSIX tar archiving used in `dar`
- [`doc/`](./doc/); Official specification for DeCAF and its supporting documentation

## Rationale

Why do we need DeCAF?

## Planned Work

- Official DeCAF implementation in C
- Stable C-api for the Rust implementation (for binding to higher level languages)
- Official DeCAF implementations in Go, Java, and C#
