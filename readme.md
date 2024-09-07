# Deterministic Compressed Archive Format (DeCAF)

The Deterministic Compressed Archive Format (DeCAF) is an archive file format that offers a large number of benefits over existing archive formats:

- 9x faster archiving and compression (vs `tar -czvf`)
- Fully deterministic, platform agnostic archives; `one set of files == one archive`, regardless of platform, operating system, etc.
- Incremental Decompression; if you only need one file, you only have to read and decompress a small section of the archive
- Built on modern standards, including `zstd` and `crc32`
