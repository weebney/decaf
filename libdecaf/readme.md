# libdecaf

In the vast majority of cases, you are looking for `decaf-capi`, which provides C bindings to the DeCAF reference implementation. This is instead a reimplementation of DeCAF in pure C, useful if you want a DeCAF implementation that doesn't need to be built with the Rust toolchain.

## Building

`make` is the only officially supported build tool for libdecaf. When your system permits, you should build libdecaf with make. It targets POSIX makefile syntax, so it should work on any system which supports this syntax including BSD and GNU make.

Dependencies are vendored in the `external/` directory.
