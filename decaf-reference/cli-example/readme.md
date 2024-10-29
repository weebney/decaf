# DeCAF Reference Implementation CLI Example

This is a minimal CLI demonstrating practical usage of the DeCAF reference implementation.

```console
USAGE: cli-example {DIRECTORY PATH | ARCHIVE PATH}
If a directory is passed, it is archived to `./DIRECTORY_NAME.df`
If an archive is passed, it is extracted to `./ARCHIVE_NAME/`
`cli-example ./samples.df` will create a directory `./samples/`
`cli-example /home/jeff/photos/` will create an archive file `./photos.df`
```

## Building

```console
$ go build .
```

This will build the CLI as an executable, `cli-example`.
