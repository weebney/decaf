package decaf_reference

import (
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"slices"
	"testing"
)

func TestEndToEnd(t *testing.T) {
	tempDir, err := os.MkdirTemp("", "decaf-TestEndToEnd-*")
	if err != nil {
		t.Errorf("setting up temporary directory failed: %s", err)
	}
	defer os.RemoveAll(tempDir)

	archive, err := Archive("./testdata/toybox-0.8.11/")
	if err != nil {
		t.Errorf("archiving failed: %s", err)
	}

	err = Unarchive(archive, tempDir)
	if err != nil {
		t.Errorf("unarchiving failed: %s", err)
	}
}

func TestArchivingAllCases(t *testing.T) {
	want, err := os.ReadFile("./testdata/all_cases_known_good.df")
	if err != nil {
		t.Errorf("reading known_good.df failed: %s", err)
	}

	got, err := Archive("./testdata/all_cases/")
	if err != nil {
		t.Errorf("archiving failed: %s", err)
	}

	if len(want) != len(got) {
		t.Errorf("got and want are not same length: len(got) = %d, len(want) = %d", len(got), len(want))
	}
	for i := range got {
		if got[i] != want[i] {
			t.Errorf("got != want")
		}
	}
}

func TestUnarchivingAllCases(t *testing.T) {
	tempDir, err := os.MkdirTemp("", "decaf-TestUnarchivingAllCases-*")
	if err != nil {
		t.Errorf("setting up temporary directory failed: %s", err)
	}
	defer os.RemoveAll(tempDir)

	archive, err := os.ReadFile("./testdata/all_cases_known_good.df")
	if err != nil {
		t.Errorf("reading test archive from testdata failed: %s", err)
	}

	err = Unarchive(archive, tempDir)
	if err != nil {
		t.Errorf("unarchiving failed: %s", err)
	}

	wants, err := getDiffInfos("./testdata/all_cases_known_good_extracted/")
	if err != nil {
		t.Errorf("failed to getDiffInfos for wants: %s", err)
	}
	gots, err := getDiffInfos(tempDir)
	if err != nil {
		t.Errorf("failed to getDiffInfos for gots: %s", err)
	}

	if len(wants) != len(gots) {
		t.Errorf("gots and wants are not same length: len(gots) = %d, len(wants) = %d", len(gots), len(wants))
		for _, want := range wants {
			t.Errorf("want has %s", want.path)
		}
		for _, got := range gots {
			t.Errorf("got has %s", got.path)
		}
	}

	for i := range wants {
		if wants[i].path != gots[i].path {
			t.Errorf("got path `%s`, want `%s`", gots[i].path, wants[i].path)
		}
		if wants[i].permissions != gots[i].permissions {
			t.Errorf("got perms `%d`, want `%d`", gots[i].permissions, wants[i].permissions)
		}

		if slices.Compare(wants[i].content, gots[i].content) != 0 {
			t.Errorf("contents dont match for path `%s` and `%s`", gots[i].path, wants[i].path)
			t.Errorf("`%s` and `%s`", gots[i].content, wants[i].content)
		}
	}
}

type diffInfo struct {
	path        string
	permissions uint32
	content     []byte
}

func getDiffInfos(dirPath string) ([]diffInfo, error) {
	out := []diffInfo{}
	err := filepath.WalkDir(dirPath, func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}

		if path == dirPath {
			return nil
		}

		fileInfo, err := d.Info()
		if err != nil {
			return fmt.Errorf("getting info for dirEntry failed: %s", err)
		}

		content := []byte{}
		if fileInfo.Mode()&fs.ModeSymlink == fs.ModeSymlink {
			readlink, err := os.Readlink(path)
			if err != nil {
				return fmt.Errorf("failed readlink: %s", err)
			}
			content = []byte(readlink)
		} else if !fileInfo.IsDir() {
			content, err = os.ReadFile(path)
			if err != nil {
				return fmt.Errorf("failed getting file content: %s", err)
			}
		}

		path, err = filepath.Rel(dirPath, path)
		if err != nil {
			return fmt.Errorf("making path relative failed: %s", err)
		}

		out = append(out, diffInfo{
			path:        path,
			permissions: uint32(fileInfo.Mode()),
			content:     content,
		})
		return nil
	})
	if err != nil {
		return []diffInfo{}, err
	}
	return out, nil
}

func BenchmarkArchiving(b *testing.B) {
	_, err := Archive("./testdata/toybox-0.8.11/")
	if err != nil {
		b.Errorf("encountered an error while archiving toybox corpus: %s", err)
	}
	b.StopTimer()
}

func BenchmarkUnarchiving(b *testing.B) {
	tempDir, err := os.MkdirTemp("", "decaf-BenchmarkUnarchiving-*")
	if err != nil {
		b.Errorf("setting up temporary directory failed: %s", err)
	}
	defer os.RemoveAll(tempDir)

	archive, err := os.ReadFile("./testdata/toybox-0.8.11.df")
	if err != nil {
		b.Errorf("reading toybox.df failed: %s", err)
	}

	b.ResetTimer()
	err = Unarchive(archive, tempDir)
	if err != nil {
		b.Errorf("encountered an error while unarchiving toybox corpus: %s", err)
	}
}
