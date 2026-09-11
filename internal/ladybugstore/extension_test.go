package ladybugstore

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/version"
)

func isolateRuntime(t *testing.T) {
	t.Helper()
	t.Setenv("HOME", t.TempDir())
	t.Setenv("USERPROFILE", t.TempDir())
}

func TestExtensionPathIsUnderTheVersionedRuntimeDir(t *testing.T) {
	isolateRuntime(t)
	got := ExtensionPath(ExtHTTPFS)

	want := filepath.Join(brand.RuntimeDir(version.Version), "lbug", "httpfs.lbug_extension")
	if got != want {
		t.Fatalf("ExtensionPath = %q, want %q", got, want)
	}
}

func TestLoadExtensionsWithoutThePayloadNamesTheMissingFile(t *testing.T) {
	isolateRuntime(t)
	st, err := Open(filepath.Join(t.TempDir(), "db"))
	if err != nil {
		t.Fatalf("open: %v", err)
	}
	defer st.Close()

	err = st.LoadExtensions(ExtHTTPFS)
	if err == nil {
		t.Fatal("LoadExtensions succeeded with no extension on disk")
	}
	if !strings.Contains(err.Error(), "httpfs.lbug_extension") {
		t.Fatalf("error does not name the missing file: %v", err)
	}
	if !strings.Contains(err.Error(), "launcher payload") {
		t.Fatalf("error does not say where the file should have come from: %v", err)
	}
}

func TestValidateExtensionFileRejectsWhatAFailedDownloadLeaves(t *testing.T) {
	dir := t.TempDir()

	errorPage := filepath.Join(dir, "httpfs.lbug_extension")
	if err := os.WriteFile(errorPage, []byte("<html>404 Not Found</html>"), 0o644); err != nil {
		t.Fatal(err)
	}
	err := validateExtensionFile(errorPage)
	if err == nil {
		t.Fatal("validateExtensionFile accepted a 404 error page")
	}
	if !strings.Contains(err.Error(), "too small") {
		t.Fatalf("error should point at the size: %v", err)
	}

	bigButWrong := filepath.Join(dir, "big.lbug_extension")
	padding := make([]byte, minExtensionBytes+1)
	copy(padding, "<!DOCTYPE html>")
	if err := os.WriteFile(bigButWrong, padding, 0o644); err != nil {
		t.Fatal(err)
	}
	if err := validateExtensionFile(bigButWrong); err == nil {
		t.Fatal("validateExtensionFile accepted a large file that is not a native library")
	}
}
