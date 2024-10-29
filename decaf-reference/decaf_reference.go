// Package decaf_reference is the reference implementation for the Deterministic Compressed Archive Format (DeCAF). Because this is a reference implementation, it should NEVER be imported into a production codebase.
package decaf_reference

import (
	"encoding/binary"
	"errors"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"slices"
	"strings"

	"github.com/DataDog/zstd"
	"github.com/bytedance/gopkg/util/xxhash3"
)

type Listing struct {
	// The total length in bytes of this Listing when written to the listing header.
	// This is easily calculated by adding the length of the path to 48; 48 being the
	// number of bytes in all the other fields.
	totalLength uint16

	// The index of the bundle the file's content is written to
	bundleIndex uint64

	// The content offset within the uncompressed bundle
	bundleOffset uint64

	// The size of the file's content
	contentSize uint64

	// The XXH3-64 checksum of the file's content
	checksum uint64

	// The mode of the file, which can be between 0 and 3
	// Values which are >3 are invalid and will cause an error
	// 0 denotes a regular file
	// 1 denotes an executable file
	// 2 denotes a link to another file
	// 3 denotes a bare directory
	mode uint8

	// The path relative to the apex, which is the root of the archive
	path string

	// Everything above here is information written directly into the header;
	// below, is the content, which is written into a bundle and compressed.

	// The binary fileContent of the file, which we'll write into a bundle
	fileContent []byte
}

type Bundle struct {
	offsetInDataSection  uint64
	compressedSize       uint64
	uncompressedChecksum uint64

	// Everything above here is information written directly into the header;
	// below, is the compressed data of the listings who store their content
	// in this bundle.
	data []byte
}

const (
	ModeNormal     uint8 = iota // 0, normal files
	ModeExecutable              // 1, executable files
	ModeLink                    // 2, link files
	ModeBareDir                 // 3, empty directories
)

func Archive(inputDirectoryPath string) ([]byte, error) {
	// Implementation specific, but to allow relative paths to be
	// passed into this function, we first need to make that path absolute
	inputDirectoryPath, err := filepath.Abs(inputDirectoryPath)
	if err != nil {
		return nil, fmt.Errorf("failed to make absolute path for path `%s`: %s", inputDirectoryPath, err)
	}

	// First, we have to gather the required information from the filesystem to construct listings
	listings := []*Listing{}
	err = filepath.WalkDir(inputDirectoryPath, func(path string, dirEntry fs.DirEntry, err error) error {
		// This just allows us to pass the errors up the call stack
		if err != nil {
			return err
		}

		// We'll start by getting the fileInfo of the file
		fileInfo, err := dirEntry.Info()
		if err != nil {
			return fmt.Errorf("failed to get info for dirEntry `%s`: %s", dirEntry.Name(), err)
		}

		// Now we can collect the necessary metadata to construct listings
		listingMode := uint8(0)
		switch {
		case fileInfo.IsDir():
			// We only care about directories that have no children (i.e. empty/bare directories);
			// all other directories exist implicitly as far as DeCAF is concerned.
			subEntries, err := os.ReadDir(path+"/fasfasfasf")
			if err != nil {
				return fmt.Errorf("failed to read directory `%s`: %s", path, err)
			}
			// If this directory has children, skip it
			if len(subEntries) > 1 {
				return nil
			}
			listingMode = ModeBareDir // ModeBareDir == 3
		case fileInfo.Mode()&fs.ModeSymlink != 0:
			// File is a symlink
			listingMode = ModeLink // ModeLink == 2
		case fileInfo.Mode()&1<<6 != 0:
			// File is executable
			listingMode = ModeExecutable // ModeExecutable == 1
		default:
			// File is normal
			listingMode = ModeNormal // ModeNormal == 0
		}

		// Links and bare directories maintain an empty content and checksum of 0
		fileContent := []byte{}
		contentChecksum := uint64(0)
		if listingMode == ModeNormal || listingMode == ModeExecutable {
			// Get the content of the file off the disk for normal and executable files
			fileContent, err = os.ReadFile(path)
			if err != nil {
				return fmt.Errorf("failed to read file `%s` with mode %d: %s", path, listingMode, err)
			}
			contentChecksum = xxhash3.Hash(fileContent)
		} else if listingMode == ModeLink {
			// Handle symlinks, setting their fileContent to the path of the listing we want
			readLink, err := os.Readlink(path)
			if err != nil {
				return fmt.Errorf("failed to readlink for `%s`: %s", path, err)
			}

			// normalize the link target
			readLink = filepath.Clean(readLink)

			// We ignore symlinks that point outside the scope of the archive
			readLink = filepath.Join(inputDirectoryPath, readLink)
			if !strings.HasPrefix(readLink, inputDirectoryPath) {
				return nil
			}

			// We ignore symlinks that point to other symlinks or files that dont exist
			readLinkInfo, err := os.Lstat(readLink)
			if errors.Is(err, os.ErrNotExist) {
				return nil
			} else if err != nil {
				return fmt.Errorf("failed to Lstat for `%s` from `%s`: %s", readLink, path, err)
			}
			if readLinkInfo.Mode()&fs.ModeSymlink != 0 {
				return nil
			}

			// Finally, we can write the readlink into the fileContent
			relativeReadlink, err := filepath.Rel(inputDirectoryPath, readLink)
			if err != nil {
				return fmt.Errorf("failed to get relative path for readlink `%s` for path `%s`: %s", readLink, path, err)
			}
			fileContent = []byte(relativeReadlink)
		}

		// We get the final relative path that will be written into the listing
		relativePath, err := filepath.Rel(inputDirectoryPath, path)
		if err != nil {
			return fmt.Errorf("failed to get relative path for path `%s`: %s", path, err)
		}

		// Partially construct a listing based on the information we've gathered
		// These are only partially constructed because they are missing the bundle
		// information, which is generated in the next step.
		listing := Listing{
			totalLength: uint16(len(relativePath)) + 35, // 38 is the size of the written listing with no path
			path:        relativePath,
			contentSize: uint64(len(fileContent)),
			checksum:    contentChecksum,
			mode:        listingMode,

			fileContent: fileContent,
		}

		// Push a pointer to the partially constructed listing into the `listings` slice
		listings = append(listings, &listing)
		return nil
	})
	if err != nil {
		return nil, fmt.Errorf("failed to walk the filepath for %s: %s", inputDirectoryPath, err)
	}

	// Next, we have to sort the listings
	slices.SortFunc(listings, func(a, b *Listing) int {
		// First, we sort in ascending order by file size
		if a.contentSize != b.contentSize {
			if a.contentSize > b.contentSize {
				return 1
			} else {
				return -1
			}
		}
		// If there are conflicts, we compare by path length
		if a.totalLength != b.totalLength {
			if a.totalLength > b.totalLength {
				return 1
			} else {
				return -1
			}
		}
		// If there are still conflicts, we compare the path byte-by-byte
		if a.path != b.path {
			// Under the hood, this just calls strcmp
			return strings.Compare(a.path, b.path)
		}
		panic("Encountered unsortable files!")
	})

	// Next, we'll compute bundle information for the listings
	const targetBundleSize = 10 * (1024 * 1024) // This is in bytes; the target bundle size is 10 MiB
	currentBundleIndex := uint64(0)
	currentBundleSize := uint64(0)
	for i, listing := range listings {
		if currentBundleSize > targetBundleSize || i == len(listings)-1 {
			currentBundleIndex += 1
			currentBundleSize = 0
		}
		listing.bundleOffset = currentBundleSize
		currentBundleSize += listing.contentSize
		listing.bundleIndex = currentBundleIndex
	}

	// The maximum bundle index should be the last currentBundleIndex
	// Bundles are indexed by 0, so we need to add 1 to get the total number of bundles
	bundlesNeeded := currentBundleIndex + 1

	// Now we can gather up the file contents and prepare them to be turned into bundles
	uncompressedBundleContents := [][]byte{}
	for range bundlesNeeded {
		// We're going to create an empty []byte for each bundle to be filled directly
		uncompressedBundleContents = append(uncompressedBundleContents, []byte{})
	}

	for _, listing := range listings {
		// Now, we can directly fill the uncompressed bundle buffers with content directly
		uncompressedBundleContents[listing.bundleIndex] = append(uncompressedBundleContents[listing.bundleIndex], listing.fileContent...)
	}

	// Generate the bundle header info and compress the bundles
	bundles := []*Bundle{}
	currentOffsetInDataSection := uint64(0)
	for i, uncompressedData := range uncompressedBundleContents {
		// Get the checksum
		uncompressedChecksum := xxhash3.Hash(uncompressedData)

		// Then, we can compress the bundle
		compressedBundleData, err := zstd.CompressLevel(nil, uncompressedData, 3)
		if err != nil {
			return nil, fmt.Errorf("failed to compress bundle with index %v: %s", i, err)
		}

		// Now, we can construct a bundle header struct for the bundle
		bundleHeaderEntry := Bundle{
			offsetInDataSection:  currentOffsetInDataSection,
			uncompressedChecksum: uncompressedChecksum,
			compressedSize:       uint64(len(compressedBundleData)),
			data:                 compressedBundleData,
		}

		// Update the current offset
		currentOffsetInDataSection += uint64(len(compressedBundleData))

		// Push a pointer to the constructed bundle header into the compressed bundles slice
		bundles = append(bundles, &bundleHeaderEntry)
	}

	// Now we can start creating portions of the final archive
	// Up first, we'll construct the listing header
	listingHeaderBuffer := []byte{}
	for _, listing := range listings {
		listingHeaderBuffer = binary.LittleEndian.AppendUint16(listingHeaderBuffer, listing.totalLength)
		listingHeaderBuffer = binary.LittleEndian.AppendUint64(listingHeaderBuffer, listing.bundleIndex)
		listingHeaderBuffer = binary.LittleEndian.AppendUint64(listingHeaderBuffer, listing.bundleOffset)
		listingHeaderBuffer = binary.LittleEndian.AppendUint64(listingHeaderBuffer, listing.contentSize)
		listingHeaderBuffer = binary.LittleEndian.AppendUint64(listingHeaderBuffer, listing.checksum)
		listingHeaderBuffer = append(listingHeaderBuffer, listing.mode)            // the mode is only one byte, so it has no endianness
		listingHeaderBuffer = append(listingHeaderBuffer, []byte(listing.path)...) // UTF-8 strings have no endianness
	}

	// Next, the bundle header
	bundleHeaderBuffer := []byte{}
	for _, bundle := range bundles {
		bundleHeaderBuffer = binary.LittleEndian.AppendUint64(bundleHeaderBuffer, bundle.offsetInDataSection)
		bundleHeaderBuffer = binary.LittleEndian.AppendUint64(bundleHeaderBuffer, bundle.compressedSize)
		bundleHeaderBuffer = binary.LittleEndian.AppendUint64(bundleHeaderBuffer, bundle.uncompressedChecksum)
	}

	// Next, we'll construct the meta header
	listingHeaderSize := uint64(len(listingHeaderBuffer))
	listingCount := uint64(len(listings))
	bundleCount := uint64(len(bundles))

	// Now, we can write all of that data into the buffer
	metaHeaderBuffer := []byte{}
	metaHeaderBuffer = binary.LittleEndian.AppendUint64(metaHeaderBuffer, listingHeaderSize)
	metaHeaderBuffer = binary.LittleEndian.AppendUint64(metaHeaderBuffer, listingCount)
	metaHeaderBuffer = binary.LittleEndian.AppendUint64(metaHeaderBuffer, bundleCount)

	// Almost there! Now we can build the data section...
	dataSectionBuffer := []byte{}
	for _, bundle := range bundles {
		dataSectionBuffer = append(dataSectionBuffer, bundle.data...)
	}

	// Finally, we can write the finished archive
	archive := []byte{}

	// We'll write the header section, which is comprised of the meta, listing, and bundle headers
	archive = append(archive, metaHeaderBuffer...)
	archive = append(archive, listingHeaderBuffer...)
	archive = append(archive, bundleHeaderBuffer...)

	// Then, we can write the data section...
	archive = append(archive, dataSectionBuffer...)

	// But before we're done, we need to get the checksum of the nearly completed archive
	archiveChecksum := xxhash3.Hash(archive)
	// And then prepend the magic number, then the checksum to the archive
	const magicNumber uint64 = 0x66616365646D6169 // "iamdecaf"
	prependBuffer := []byte{}
	prependBuffer = binary.LittleEndian.AppendUint64(prependBuffer, magicNumber)
	prependBuffer = binary.LittleEndian.AppendUint64(prependBuffer, archiveChecksum)
	archive = append(prependBuffer, archive...)

	// Et voilà!
	return archive, nil
}

func Unarchive(archive []byte, outputDirectoryPath string) error {
	magic := binary.LittleEndian.Uint64(archive[0:8])
	if magic != 0x66616365646D6169 {
		panic("bad magic")
	}
	cksumExtracted := binary.LittleEndian.Uint64(archive[8:16])
	if cksumExtracted != xxhash3.Hash(archive[16:]) {
		panic("bad archive cksum")
	}

	listingHeaderSize := binary.LittleEndian.Uint64(archive[16:24])
	listingCount := binary.LittleEndian.Uint64(archive[24:32])
	bundleCount := binary.LittleEndian.Uint64(archive[32:40])

	listingHeaderStartOffset := uint64(40)
	bundleHeaderStartOffset := listingHeaderStartOffset + listingHeaderSize
	bundleHeaderSize := bundleCount * 24
	dataSectionStartOffset := bundleHeaderStartOffset + bundleHeaderSize

	listingHeader := archive[listingHeaderStartOffset:bundleHeaderStartOffset]
	bundleHeader := archive[bundleHeaderStartOffset:dataSectionStartOffset]
	dataSection := archive[dataSectionStartOffset:]

	bundleHeaderCursor := uint64(0)
	bundles := []*Bundle{}
	for range bundleCount {
		bundleOffsetInDataSection := binary.LittleEndian.Uint64(bundleHeader[bundleHeaderCursor : bundleHeaderCursor+8])
		bundleCompressedSize := binary.LittleEndian.Uint64(bundleHeader[bundleHeaderCursor+8 : bundleHeaderCursor+16])
		bundleExpectedChecksum := binary.LittleEndian.Uint64(bundleHeader[bundleHeaderCursor+16 : bundleHeaderCursor+24])

		bundleHeaderCursor += 24

		bundleCompressedData := dataSection[bundleOffsetInDataSection : bundleOffsetInDataSection+bundleCompressedSize]

		bundleData, err := zstd.Decompress([]byte{}, bundleCompressedData)
		if err != nil {
			panic("failed to decompress bundle")
		}

		if bundleExpectedChecksum != xxhash3.Hash(bundleData) {
			panic("bad bundle checksum")
		}

		bundle := Bundle{
			offsetInDataSection:  bundleOffsetInDataSection,
			compressedSize:       bundleCompressedSize,
			uncompressedChecksum: bundleExpectedChecksum,
			data:                 bundleData,
		}

		bundles = append(bundles, &bundle)
	}

	listings := []*Listing{}
	for range listingCount {
		listingLength := binary.LittleEndian.Uint16(listingHeader[:2])
		listingBundleIndex := binary.LittleEndian.Uint64(listingHeader[2:10])
		listingBundleOffset := binary.LittleEndian.Uint64(listingHeader[10:18])
		listingContentSize := binary.LittleEndian.Uint64(listingHeader[18:26])
		listingExpectedChecksum := binary.LittleEndian.Uint64(listingHeader[26:34])
		listingMode := uint8(listingHeader[34])
		listingPath := string(listingHeader[35:listingLength])
		listingHeader = listingHeader[listingLength:]

		fileContent := []byte{}
		if listingMode == ModeNormal || listingMode == ModeExecutable || listingMode == ModeLink {
			fileContent = bundles[listingBundleIndex].data[listingBundleOffset : listingBundleOffset+listingContentSize]
		}
		if (listingMode == ModeNormal || listingMode == ModeExecutable) && listingExpectedChecksum != xxhash3.Hash(fileContent) {
			panic("bad listing checksum")
		}

		listing := Listing{
			totalLength:  listingLength,
			bundleIndex:  listingBundleIndex,
			bundleOffset: listingBundleOffset,
			contentSize:  listingContentSize,
			checksum:     listingExpectedChecksum,
			mode:         listingMode,
			path:         listingPath,
			fileContent:  fileContent,
		}
		listings = append(listings, &listing)
	}

	// Now, we can create the files for all the listings
	for _, listing := range listings {

		// Ensure we're placing files into our new directory
		listingPath := filepath.Join(outputDirectoryPath, listing.path)

		// Non-bare directories are created implicitly here
		listingParentPath := filepath.Dir(listingPath)
		err := os.MkdirAll(listingParentPath, 0o100755)
		if err != nil {
			panic(err)
		}

		// If this listing is a bare directory, we need to create it
		if listing.mode == ModeBareDir {
			err := os.MkdirAll(listingPath, 0o100755)
			if err != nil {
				panic(err)
			}
			continue
		}

		// If this listing is a link, we need to create it as a symlink
		// The link target is stored in the fileContent
		if listing.mode == ModeLink {
			targetPath := string(listing.fileContent)
			err := os.Symlink(targetPath, listingPath)
			if err != nil {
				panic(err)
			}
			continue
		}

		// For everything else, we need to actually create a file
		file, err := os.Create(listingPath)
		if err != nil {
			panic(err)
		}

		// Set the unix permissions (st_mode)
		unixMode := 0o100644
		if listing.mode == ModeExecutable {
			unixMode = 0o100755
		}
		err = file.Chmod(fs.FileMode(unixMode))
		if err != nil {
			panic(err)
		}

		// Finally, we fill the file with its content
		_, err = file.Write(listing.fileContent)
		if err != nil {
			panic(err)
		}
	}

	return nil
}
