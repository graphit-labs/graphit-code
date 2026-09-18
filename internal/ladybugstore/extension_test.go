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

// The engine reaches a plain-HTTP endpoint only when it is told to, and a broker serving its own
// storage over loopback is exactly that case: without s3_disable_ssl httpfs tries HTTPS against
// it and the read fails.
func TestS3ConfigStatementsCarryPathStyleAndPlainHTTP(t *testing.T) {
	statements := S3ConfigStatements(S3Credentials{
		AccessKeyID: "key", SecretAccessKey: "secret", SessionToken: "token",
		Region: "us-east-1", Endpoint: "127.0.0.1:8080", PathStyle: true, DisableSSL: true,
	})
	joined := strings.Join(statements, "\n")
	for _, want := range []string{
		"CALL s3_access_key_id='key'",
		"CALL s3_secret_access_key='secret'",
		"CALL s3_session_token='token'",
		"CALL s3_region='us-east-1'",
		"CALL s3_endpoint='127.0.0.1:8080'",
		"CALL s3_url_style='path'",
		"CALL s3_disable_ssl=true",
	} {
		if !strings.Contains(joined, want) {
			t.Errorf("missing %s in:\n%s", want, joined)
		}
	}

	// An HTTPS bucket with no session token must not be told either of those.
	plain := strings.Join(S3ConfigStatements(S3Credentials{
		AccessKeyID: "key", SecretAccessKey: "secret", Region: "us-east-1",
	}), "\n")
	for _, unwanted := range []string{"s3_session_token", "s3_endpoint", "s3_url_style", "s3_disable_ssl"} {
		if strings.Contains(plain, unwanted) {
			t.Errorf("unexpected %s in:\n%s", unwanted, plain)
		}
	}
}
