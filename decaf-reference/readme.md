# DeCAF Reference Implementation

Special care was taken to make this implementation easy to follow. This implementation uses as few Go-specific features as possible; therefore, only a minimal knowledge of Go is necessary to understand what it is doing at any given point. There are only two functions provided by this library: `Archive()` and `Unarchive()`. These provide a completely imperative implementation of both major operations for the format; they continue line-by-line with heavy commenting to make it clear what any section of code is doing. They also use no Go-style writers; all writing of data is done sequentially into "slices" of bytes (i.e. `[]byte`), which are analagous to dynamically sized arrays/vectors/arraylists provided by any high-level language. Many parts of this implementation are obviously redundant, but they are redundant such that any section of code can be easily understood and applied to another language, then optimized in that language using language-specific constructs.

## Examples

An example implementation of a minimal CLI can be found in the `/examples/cli` directory; the package is also directly used in the `decaf_reference_test.go` file. Because this is the reference implementation, it should never be imported into a production codebase.

## Why Go?

A reference implementation ideally targets correctness and ease of understanding (as opposed to, say, performance); it is no more than a fully executable piece of documentation used as a supplement to the specification. Go might seem like an interesting choice for a reference implementation, but it was chosen for the following reasons:

- It is a "low" enough level language that an implementation written in it still provides applicable detail for other "lower level" languages.
- It is a "high" enough level language to prevent inclusion of code that would distract from the actual implementation details, i.e. `malloc` and `free`.
- The limited syntax makes it extremely easy to understand while still providing the necessary tools for verifying that the implementation is completely correct. Even if you aren't familiar with Go, the implementation should be very easy to understand.
- An official implementation of DeCAF is available in Rust, which high-level languages should bind to directly. Binding to the reference implementation in production code would be an anti-pattern because the reference implementation does not prioritize performance, only correctness and readability. Go prevents this because it can not be bound to from other high-level languages.
- The tightly integrated toolchain adds no overhead to testing.
- The large standard library means that the only external dependencies required are bindings to implementations of Zstandard and Blake3.

## Testing & Benchmarking

This package uses Go's built in testing. It utilizes a handmade corpus that covers all possible cases to verify correctness. To test the package:

```console
$ go test
```

The source tree of Rob Landley's [toybox 0.8.11](https://github.com/landley/toybox/tree/0.8.11) is distributed with this repository and utilized for benchmarking, chosen for its ideal repository size and `0BSD` license. To benchmark the package:

```console
$ go test -bench='.'
```
