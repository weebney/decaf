# DeCAF command line utility

## TAR Mode

The DeCAF command line utility can also be used as a drop in replacement for the `tar` command to produce deterministic tarballs (compatible with bsdtar) ~5x faster than the `tar` utility.

Just prepend `df` to the `tar` command you want to run.

`df tar -czvf`
