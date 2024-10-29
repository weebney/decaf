package main

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	decaf_reference "github.com/weebney/decaf/decaf-reference"
)

func usage() {
	executable := os.Args[0]
	fmt.Printf("USAGE: %s {DIRECTORY PATH | ARCHIVE PATH}\n", executable)
	fmt.Printf("If a directory is passed, it is archived to `./DIRECTORY_NAME.df`\n")
	fmt.Printf("If an archive is passed, it is extracted to `./ARCHIVE_NAME/`\n")
	fmt.Printf("`%s ./samples.df` will create a directory `./samples/`\n", executable)
	fmt.Printf("`%s /home/jeff/photos/` will create an archive file `./photos.df`\n", executable)
}

func main() {
	args := os.Args

	// not enough args or too many args
	if len(args) < 2 || len(args) > 2 {
		usage()
		os.Exit(1)
	}

	inputPath := args[1]
	stat, err := os.Stat(inputPath)
	if err != nil {
		fmt.Printf("Failed to stat path `%s`: %s\n", inputPath, err)
		os.Exit(2)
	}

	start := time.Now()

	if stat.IsDir() {
		// if the input path is a dir, we're making an archive
		outputArchivePath := filepath.Base(inputPath) + ".df"
		fmt.Printf("Creating an archive from directory `%s` to `%s`\n", inputPath, outputArchivePath)
		err = ArchiveDirectoryToFile(inputPath, outputArchivePath)
		if err != nil {
			fmt.Printf("Failed to archive from path `%s`: %s\n", inputPath, err)
			os.Exit(4)
		}
		fmt.Printf("Successfully archived from `%s` to `%s` (took %.2f s)\n", inputPath, outputArchivePath, time.Since(start).Seconds())
	} else {
		// if not, we're unarchiving the directory at the path
		outputDirPath := strings.TrimSuffix(filepath.Base(inputPath), ".df")
		fmt.Printf("Creating a directory from archive `%s` to `%s`\n", inputPath, outputDirPath)
		err = UnarchiveFileToDirectory(inputPath, outputDirPath)
		if err != nil {
			fmt.Printf("Failed to unarchive from path `%s`: %s\n", inputPath, err)
			os.Exit(5)
		}
		fmt.Printf("Successfully unarchived from `%s` to `%s` (took %.2f s)\n", inputPath, outputDirPath, time.Since(start).Seconds())

	}
	// implicitly exits with 0
}

func ArchiveDirectoryToFile(directoryPath string, outputFilePath string) error {
	archive, err := decaf_reference.Archive(directoryPath)
	if err != nil {
		return fmt.Errorf("failed to archive directory `%s`: %s", directoryPath, err)
	}

	outFile, err := os.Create(outputFilePath)
	_, err = outFile.Write(archive)
	if err != nil {
		return fmt.Errorf("failed to create output file `%s`: %s", outputFilePath, err)
	}

	return nil
}

func UnarchiveFileToDirectory(archivePath string, outputDirectoryPath string) error {
	archiveBytes, err := os.ReadFile(archivePath)
	if err != nil {
		return fmt.Errorf("failed to read archive file `%s`: %s", archivePath, err)
	}

	err = decaf_reference.Unarchive(archiveBytes, outputDirectoryPath)
	if err != nil {
		return fmt.Errorf("failed to unarchive to `%s`: %s", outputDirectoryPath, err)
	}

	return nil
}
