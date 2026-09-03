package ladybugstore

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/version"
)

const makefileExtensionCache = "/tmp/lbug-extension-cache"

func realHTTPFSBinary(t *testing.T) string {
	t.Helper()
	matches, _ := filepath.Glob(filepath.Join(makefileExtensionCache, "*", "*", "httpfs.lbug_extension"))
	for _, m := range matches {
		if info, err := os.Stat(m); err == nil && info.Size() > 1<<20 {
			return m
		}
	}
	t.Skip("no httpfs extension in " + makefileExtensionCache + " — run a platform build first")
	return ""
}

func isolateRuntime(t *testing.T) {
	t.Helper()
	t.Setenv("HOME", t.TempDir())
	t.Setenv("USERPROFILE", t.TempDir())
}

func stageExtension(t *testing.T, source string) {
	t.Helper()
	dir := ExtensionDir()
	if dir == "" {
		t.Fatal("ExtensionDir is empty — the test home is not set up")
	}
	if err := os.MkdirAll(dir, 0o755); err != nil {
		t.Fatal(err)
	}
	data, err := os.ReadFile(source)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(ExtensionPath(ExtHTTPFS), data, 0o644); err != nil {
		t.Fatal(err)
	}
}

func TestExtensionPathIsUnderTheVersionedRuntimeDir(t *testing.T) {
	isolateRuntime(t)
	got := ExtensionPath(ExtHTTPFS)

	want := filepath.Join(brand.RuntimeDir(version.Version), "lbug", "httpfs.lbug_extension")
	if got != want {
		t.Fatalf("ExtensionPath = %q, want %q", got, want)
	}
}

// The whole point of carrying the extension in the launcher: it loads from a file, with no
// INSTALL and no network.
func TestLoadExtensionsFromTheLauncherPayload(t *testing.T) {
	isolateRuntime(t)
	source := realHTTPFSBinary(t)
	stageExtension(t, source)

	st, err := Open(filepath.Join(t.TempDir(), "db"))
	if err != nil {
		t.Fatalf("open: %v", err)
	}
	defer st.Close()

	if err := st.LoadExtensions(ExtHTTPFS); err != nil {
		t.Fatalf("LoadExtensions: %v", err)
	}

	loaded, err := st.LoadedExtensions()
	if err != nil {
		t.Fatalf("LoadedExtensions: %v", err)
	}
	if !containsFold(loaded, ExtHTTPFS) {
		t.Fatalf("loaded = %v; want it to contain %s", loaded, ExtHTTPFS)
	}
}

// A missing payload must name the path. This is the failure an air-gapped or half-extracted
// installation actually produces, and it needs no extension binary to test.
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

func TestValidateExtensionFileAcceptsTheRealBinary(t *testing.T) {
	source := realHTTPFSBinary(t)

	if err := validateExtensionFile(source); err != nil {
		t.Fatalf("validateExtensionFile rejected the published extension: %v", err)
	}
}

func TestConfigureS3IssuesTheOptionFormNotTheDocumentedFunction(t *testing.T) {
	isolateRuntime(t)
	source := realHTTPFSBinary(t)
	stageExtension(t, source)

	st, err := Open(filepath.Join(t.TempDir(), "db"))
	if err != nil {
		t.Fatalf("open: %v", err)
	}
	defer st.Close()

	if err := st.LoadExtensions(ExtHTTPFS); err != nil {
		t.Fatalf("LoadExtensions: %v", err)
	}

	if err := st.ConfigureS3(S3Credentials{
		AccessKeyID:     "AKIAEXAMPLE",
		SecretAccessKey: "secret-example",
		Region:          "us-east-1",
		Endpoint:        "localhost:9000",
		PathStyle:       true,
	}); err != nil {
		t.Fatalf("ConfigureS3: %v", err)
	}
}

// An empty field must be skipped rather than sent as an empty option: on the AWS default
// chain a session token is frequently absent, and setting it to "" is not the same as not
// setting it.
func TestConfigureS3SkipsEmptyValues(t *testing.T) {
	isolateRuntime(t)
	source := realHTTPFSBinary(t)
	stageExtension(t, source)

	st, err := Open(filepath.Join(t.TempDir(), "db"))
	if err != nil {
		t.Fatalf("open: %v", err)
	}
	defer st.Close()

	if err := st.LoadExtensions(ExtHTTPFS); err != nil {
		t.Fatalf("LoadExtensions: %v", err)
	}

	if err := st.ConfigureS3(S3Credentials{Region: "sa-east-1"}); err != nil {
		t.Fatalf("ConfigureS3 with only a region: %v", err)
	}
}

// A quote in a credential or an endpoint must not end the Cypher literal early.
func TestConfigureS3EscapesValues(t *testing.T) {
	isolateRuntime(t)
	source := realHTTPFSBinary(t)
	stageExtension(t, source)

	st, err := Open(filepath.Join(t.TempDir(), "db"))
	if err != nil {
		t.Fatalf("open: %v", err)
	}
	defer st.Close()

	if err := st.LoadExtensions(ExtHTTPFS); err != nil {
		t.Fatalf("LoadExtensions: %v", err)
	}

	if err := st.ConfigureS3(S3Credentials{AccessKeyID: `key'with"quotes`}); err != nil {
		t.Fatalf("ConfigureS3 with a quoted value: %v", err)
	}
}
