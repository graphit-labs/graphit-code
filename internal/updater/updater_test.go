package updater

import (
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

type mockRoundTripper struct {
	roundTripFunc func(req *http.Request) (*http.Response, error)
}

func (m *mockRoundTripper) RoundTrip(req *http.Request) (*http.Response, error) {
	return m.roundTripFunc(req)
}

func TestLatestRelease(t *testing.T) {
	origTransport := http.DefaultTransport
	defer func() { http.DefaultTransport = origTransport }()

	// 1. Success case
	http.DefaultTransport = &mockRoundTripper{
		roundTripFunc: func(req *http.Request) (*http.Response, error) {
			jsonResp := `{"tag_name":"v1.2.0","assets":[{"name":"graphit-linux-amd64","browser_download_url":"http://download/binary","size":12345}]}`
			return &http.Response{
				StatusCode: http.StatusOK,
				Body:       io.NopCloser(strings.NewReader(jsonResp)),
			}, nil
		},
	}

	rel, err := LatestRelease("org/repo", "")
	if err != nil {
		t.Fatalf("expected no error, got %v", err)
	}
	if rel.TagName != "v1.2.0" {
		t.Errorf("expected tag_name v1.2.0, got %s", rel.TagName)
	}
	if len(rel.Assets) != 1 || rel.Assets[0].Name != "graphit-linux-amd64" {
		t.Errorf("unexpected assets: %v", rel.Assets)
	}

	// 2. 404 Not Found case
	http.DefaultTransport = &mockRoundTripper{
		roundTripFunc: func(req *http.Request) (*http.Response, error) {
			return &http.Response{
				StatusCode: http.StatusNotFound,
				Body:       io.NopCloser(strings.NewReader("Not Found")),
			}, nil
		},
	}
	_, err = LatestRelease("org/repo", "")
	if err == nil || !strings.Contains(err.Error(), "no releases found") {
		t.Errorf("expected 404 releases error, got %v", err)
	}

	// 3. Error case
	http.DefaultTransport = &mockRoundTripper{
		roundTripFunc: func(req *http.Request) (*http.Response, error) {
			return nil, errors.New("network error")
		},
	}
	_, err = LatestRelease("org/repo", "")
	if err == nil {
		t.Error("expected network error")
	}
}

func TestLatestReleaseCustomURL(t *testing.T) {
	origTransport := http.DefaultTransport
	defer func() { http.DefaultTransport = origTransport }()

	var capturedURL string
	http.DefaultTransport = &mockRoundTripper{
		roundTripFunc: func(req *http.Request) (*http.Response, error) {
			capturedURL = req.URL.String()
			jsonResp := `{"tag_name":"v2.0.0","assets":[{"name":"custom-linux-amd64","browser_download_url":"http://custom/binary","size":99}]}`
			return &http.Response{
				StatusCode: http.StatusOK,
				Body:       io.NopCloser(strings.NewReader(jsonResp)),
			}, nil
		},
	}

	rel, err := LatestRelease("org/repo", "https://my-server.example.com/api/releases/latest")
	if err != nil {
		t.Fatalf("expected no error, got %v", err)
	}
	if rel.TagName != "v2.0.0" {
		t.Errorf("expected tag_name v2.0.0, got %s", rel.TagName)
	}
	if capturedURL != "https://my-server.example.com/api/releases/latest" {
		t.Errorf("expected custom URL, got %s", capturedURL)
	}

	// Verify trailing slash is stripped
	capturedURL = ""
	_, _ = LatestRelease("org/repo", "https://my-server.example.com/releases/")
	if capturedURL != "https://my-server.example.com/releases" {
		t.Errorf("expected trailing slash stripped, got %s", capturedURL)
	}
}

func TestPlatformBinaryName(t *testing.T) {
	name := PlatformBinaryName("graphit")
	if name == "" {
		t.Error("expected non-empty platform binary name")
	}
}

func TestFindAsset(t *testing.T) {
	rel := &Release{
		Assets: []Asset{
			{Name: "binary-1", BrowserDownloadURL: "http://url-1"},
			{Name: "binary-2", BrowserDownloadURL: "http://url-2"},
		},
	}

	url := FindAsset(rel, "binary-2")
	if url != "http://url-2" {
		t.Errorf("expected http://url-2, got %q", url)
	}

	urlMissing := FindAsset(rel, "binary-3")
	if urlMissing != "" {
		t.Errorf("expected empty string, got %q", urlMissing)
	}
}

func TestNeedsUpdate(t *testing.T) {
	tests := []struct {
		curr string
		lat  string
		want bool
	}{
		{"dev", "v1.2.0", false},
		{"v1.0.0", "v1.0.0", false},
		{"v1.0.0", "v1.1.0", true},
		{"1.0.0", "v1.0.1", true},
		{"v1.1.0", "v1.0.0", false},
		{"v1.0.0-alpha", "v1.0.0", true},
		{"v1.0.0", "v1.0.0-alpha", false},
	}

	for _, tc := range tests {
		got := NeedsUpdate(tc.curr, tc.lat)
		if got != tc.want {
			t.Errorf("NeedsUpdate(%q, %q) = %t; want %t", tc.curr, tc.lat, got, tc.want)
		}
	}
}

func TestDownloadAndChecksum(t *testing.T) {
	origTransport := http.DefaultTransport
	defer func() { http.DefaultTransport = origTransport }()

	tempDir, err := os.MkdirTemp("", "graphit-update-test-*")
	if err != nil {
		t.Fatalf("failed to create temp dir: %v", err)
	}
	defer func() { _ = os.RemoveAll(tempDir) }()

	mockContent := "my update binary content"
	h := sha256.New()
	h.Write([]byte(mockContent))
	checksumHex := hex.EncodeToString(h.Sum(nil))

	http.DefaultTransport = &mockRoundTripper{
		roundTripFunc: func(req *http.Request) (*http.Response, error) {
			resp := &http.Response{
				StatusCode:    http.StatusOK,
				Body:          io.NopCloser(strings.NewReader(mockContent)),
				ContentLength: int64(len(mockContent)),
			}
			return resp, nil
		},
	}

	destFile := filepath.Join(tempDir, "graphit-bin")

	// 1. Download
	var progressCalled bool
	err = Download("http://url/binary", destFile, func(downloaded, total int64) {
		progressCalled = true
		if downloaded != total || total != int64(len(mockContent)) {
			t.Errorf("unexpected progress callback: %d / %d", downloaded, total)
		}
	})
	if err != nil {
		t.Fatalf("Download failed: %v", err)
	}
	if !progressCalled {
		t.Error("expected progress callback to be called")
	}

	// Verify download file content
	data, _ := os.ReadFile(destFile)
	if string(data) != mockContent {
		t.Errorf("expected content %q, got %q", mockContent, string(data))
	}

	checksumFile := filepath.Join(tempDir, "checksums.sha256")
	checksumContent := checksumHex + "  graphit-bin\n"
	_ = os.WriteFile(checksumFile, []byte(checksumContent), 0644)

	err = VerifyChecksum(destFile, checksumFile)
	if err != nil {
		t.Errorf("VerifyChecksum failed: %v", err)
	}

	// 3. Test verification failure (mismatch)
	badChecksumFile := filepath.Join(tempDir, "bad_checksums.sha256")
	badChecksumContent := "badhash  graphit-bin\n"
	_ = os.WriteFile(badChecksumFile, []byte(badChecksumContent), 0644)
	err = VerifyChecksum(destFile, badChecksumFile)
	if err == nil {
		t.Error("expected VerifyChecksum to fail due to mismatch")
	}
}

func TestVerifyChecksumPerAsset(t *testing.T) {
	tempDir, err := os.MkdirTemp("", "graphit-per-asset-checksum-*")
	if err != nil {
		t.Fatalf("failed to create temp dir: %v", err)
	}
	defer func() { _ = os.RemoveAll(tempDir) }()

	content := "binary content for per-asset checksum test"
	h := sha256.New()
	h.Write([]byte(content))
	checksumHex := hex.EncodeToString(h.Sum(nil))

	binaryFile := filepath.Join(tempDir, ".graphit-update-123456")
	_ = os.WriteFile(binaryFile, []byte(content), 0644)

	// Format: "hash  filename" (standard sha256sum output)
	checksumWithName := filepath.Join(tempDir, "graphit-linux-amd64.sha256")
	_ = os.WriteFile(checksumWithName, []byte(checksumHex+"  graphit-linux-amd64\n"), 0644)

	if err := VerifyChecksum(binaryFile, checksumWithName); err != nil {
		t.Errorf("VerifyChecksum with hash+filename format failed: %v", err)
	}

	// Format: hash only
	checksumHashOnly := filepath.Join(tempDir, "hash-only.sha256")
	_ = os.WriteFile(checksumHashOnly, []byte(checksumHex+"\n"), 0644)

	if err := VerifyChecksum(binaryFile, checksumHashOnly); err != nil {
		t.Errorf("VerifyChecksum with hash-only format failed: %v", err)
	}

	// Mismatch
	badChecksum := filepath.Join(tempDir, "bad.sha256")
	_ = os.WriteFile(badChecksum, []byte("badhash  graphit-linux-amd64\n"), 0644)

	if err := VerifyChecksum(binaryFile, badChecksum); err == nil {
		t.Error("expected VerifyChecksum to fail due to mismatch")
	}

	emptyChecksum := filepath.Join(tempDir, "empty.sha256")
	_ = os.WriteFile(emptyChecksum, []byte(""), 0644)

	if err := VerifyChecksum(binaryFile, emptyChecksum); err == nil {
		t.Error("expected VerifyChecksum to fail for empty checksum file")
	}
}

func TestAtomicReplace(t *testing.T) {
	tempDir, err := os.MkdirTemp("", "graphit-replace-test-*")
	if err != nil {
		t.Fatalf("failed to create temp dir: %v", err)
	}
	defer func() { _ = os.RemoveAll(tempDir) }()

	exePath := filepath.Join(tempDir, "current-exe")
	newPath := filepath.Join(tempDir, "new-exe")

	_ = os.WriteFile(exePath, []byte("old binary content"), 0755)
	_ = os.WriteFile(newPath, []byte("new binary content"), 0755)

	err = AtomicReplace(newPath, exePath)
	if err != nil {
		t.Fatalf("AtomicReplace failed: %v", err)
	}

	// Verify exePath replaced
	data, _ := os.ReadFile(exePath)
	if string(data) != "new binary content" {
		t.Errorf("expected 'new binary content', got %q", string(data))
	}

	// Verify newPath deleted/removed
	if _, err := os.Stat(newPath); !os.IsNotExist(err) {
		t.Error("expected temporary new binary to be cleaned up/removed")
	}
}

func TestCompareVersions(t *testing.T) {
	tests := []struct {
		a, b string
		want int
	}{
		{"1.0.0", "1.0.0", 0},
		{"1.1.0", "1.0.0", 1},
		{"1.0.0", "1.1.0", -1},
		{"2.0", "1.9.9", 1},
		{"1.0.0-alpha", "1.0.0-beta", -1},
		{"1.0.0-beta", "1.0.0-alpha", 1},
		{"1.0.0", "1.0.0-alpha", 1},
		{"1.0.0-alpha", "1.0.0", -1},
	}

	for _, tc := range tests {
		got := compareVersions(tc.a, tc.b)
		if got != tc.want {
			t.Errorf("compareVersions(%q, %q) = %d; want %d", tc.a, tc.b, got, tc.want)
		}
	}
}

func TestAtomicReplaceCrossDir(t *testing.T) {
	targetDir, err := os.MkdirTemp("", "graphit-replace-target-*")
	if err != nil {
		t.Fatalf("failed to create target dir: %v", err)
	}
	defer func() { _ = os.RemoveAll(targetDir) }()

	sourceDir, err := os.MkdirTemp("", "graphit-replace-source-*")
	if err != nil {
		t.Fatalf("failed to create source dir: %v", err)
	}
	defer func() { _ = os.RemoveAll(sourceDir) }()

	exePath := filepath.Join(targetDir, "current-exe")
	newPath := filepath.Join(sourceDir, "new-exe")

	_ = os.WriteFile(exePath, []byte("old binary content"), 0755)
	_ = os.WriteFile(newPath, []byte("new binary content"), 0755)

	err = AtomicReplace(newPath, exePath)
	if err != nil {
		t.Fatalf("AtomicReplace across dirs failed: %v", err)
	}

	data, _ := os.ReadFile(exePath)
	if string(data) != "new binary content" {
		t.Errorf("expected 'new binary content', got %q", string(data))
	}

	if _, err := os.Stat(newPath); !os.IsNotExist(err) {
		t.Error("expected source file to be cleaned up")
	}
}

func TestCopyFile(t *testing.T) {
	tmpDir, err := os.MkdirTemp("", "graphit-copy-test-*")
	if err != nil {
		t.Fatalf("failed to create temp dir: %v", err)
	}
	defer func() { _ = os.RemoveAll(tmpDir) }()

	srcPath := filepath.Join(tmpDir, "src")
	dstPath := filepath.Join(tmpDir, "dst")

	content := "binary content for copy test"
	_ = os.WriteFile(srcPath, []byte(content), 0755)

	err = copyFile(srcPath, dstPath)
	if err != nil {
		t.Fatalf("copyFile failed: %v", err)
	}

	data, _ := os.ReadFile(dstPath)
	if string(data) != content {
		t.Errorf("expected %q, got %q", content, string(data))
	}

	info, _ := os.Stat(dstPath)
	if info.Mode().Perm()&0111 == 0 {
		t.Error("expected destination to be executable")
	}
}

func TestLatestReleaseNon200Status(t *testing.T) {
	origTransport := http.DefaultTransport
	defer func() { http.DefaultTransport = origTransport }()

	http.DefaultTransport = &mockRoundTripper{
		roundTripFunc: func(req *http.Request) (*http.Response, error) {
			return &http.Response{
				StatusCode: http.StatusInternalServerError,
				Body:       io.NopCloser(strings.NewReader("Internal Server Error")),
			}, nil
		},
	}

	_, err := LatestRelease("org/repo", "")
	if err == nil || !strings.Contains(err.Error(), "status 500") {
		t.Errorf("expected status 500 error, got %v", err)
	}
}

func TestLatestReleaseInvalidJSON(t *testing.T) {
	origTransport := http.DefaultTransport
	defer func() { http.DefaultTransport = origTransport }()

	http.DefaultTransport = &mockRoundTripper{
		roundTripFunc: func(req *http.Request) (*http.Response, error) {
			return &http.Response{
				StatusCode: http.StatusOK,
				Body:       io.NopCloser(strings.NewReader("not json")),
			}, nil
		},
	}

	_, err := LatestRelease("org/repo", "")
	if err == nil || !strings.Contains(err.Error(), "decoding") {
		t.Errorf("expected decoding error, got %v", err)
	}
}

func TestDownloadWithoutProgress(t *testing.T) {
	origTransport := http.DefaultTransport
	defer func() { http.DefaultTransport = origTransport }()

	content := "binary data"
	http.DefaultTransport = &mockRoundTripper{
		roundTripFunc: func(req *http.Request) (*http.Response, error) {
			return &http.Response{
				StatusCode: http.StatusOK,
				Body:       io.NopCloser(strings.NewReader(content)),
			}, nil
		},
	}

	tmpDir := t.TempDir()
	dest := filepath.Join(tmpDir, "download-noprogress")
	err := Download("http://example.com/binary", dest, nil)
	if err != nil {
		t.Fatalf("Download without progress failed: %v", err)
	}
	data, _ := os.ReadFile(dest)
	if string(data) != content {
		t.Errorf("expected %q, got %q", content, string(data))
	}
}

func TestDownloadServerError(t *testing.T) {
	origTransport := http.DefaultTransport
	defer func() { http.DefaultTransport = origTransport }()

	http.DefaultTransport = &mockRoundTripper{
		roundTripFunc: func(req *http.Request) (*http.Response, error) {
			return &http.Response{
				StatusCode: http.StatusForbidden,
				Body:       io.NopCloser(strings.NewReader("Forbidden")),
			}, nil
		},
	}

	tmpDir := t.TempDir()
	dest := filepath.Join(tmpDir, "download-forbidden")
	err := Download("http://example.com/binary", dest, nil)
	if err == nil || !strings.Contains(err.Error(), "status 403") {
		t.Errorf("expected status 403 error, got %v", err)
	}
}

func TestDownloadNetworkError(t *testing.T) {
	origTransport := http.DefaultTransport
	defer func() { http.DefaultTransport = origTransport }()

	http.DefaultTransport = &mockRoundTripper{
		roundTripFunc: func(req *http.Request) (*http.Response, error) {
			return nil, errors.New("connection refused")
		},
	}

	tmpDir := t.TempDir()
	dest := filepath.Join(tmpDir, "download-err")
	err := Download("http://example.com/binary", dest, nil)
	if err == nil || !strings.Contains(err.Error(), "downloading") {
		t.Errorf("expected download error, got %v", err)
	}
}

func TestDownloadDestFileError(t *testing.T) {
	origTransport := http.DefaultTransport
	defer func() { http.DefaultTransport = origTransport }()

	http.DefaultTransport = &mockRoundTripper{
		roundTripFunc: func(req *http.Request) (*http.Response, error) {
			return &http.Response{
				StatusCode: http.StatusOK,
				Body:       io.NopCloser(strings.NewReader("data")),
			}, nil
		},
	}

	// Dest path doesn't exist (parent dir missing)
	err := Download("http://example.com/binary", "/nonexistent/dir/file", nil)
	if err == nil || !strings.Contains(err.Error(), "creating destination file") {
		t.Errorf("expected dest file error, got %v", err)
	}
}

func TestAtomicReplaceMissingCurrent(t *testing.T) {
	tmpDir := t.TempDir()
	newPath := filepath.Join(tmpDir, "new-exe")
	_ = os.WriteFile(newPath, []byte("new"), 0755)

	// Current exe doesn't exist — Rename should fail
	err := AtomicReplace(newPath, filepath.Join(tmpDir, "nonexistent"))
	if err == nil || !strings.Contains(err.Error(), "backing up") {
		t.Errorf("expected backing up error, got %v", err)
	}
}

func TestAtomicReplaceRenameNewFails(t *testing.T) {
	tmpDir := t.TempDir()
	exePath := filepath.Join(tmpDir, "current-exe")
	_ = os.WriteFile(exePath, []byte("old"), 0755)

	// New path doesn't exist — second Rename should fail
	err := AtomicReplace(filepath.Join(tmpDir, "nonexistent-new"), exePath)
	if err == nil {
		t.Error("expected error when new binary doesn't exist")
	}
	// Original should be restored
	data, readErr := os.ReadFile(exePath)
	if readErr != nil {
		t.Errorf("expected original restored, got read error: %v", readErr)
	}
	if string(data) != "old" {
		t.Errorf("expected original content 'old', got %q", string(data))
	}
}

func TestCopyFileSourceNotFound(t *testing.T) {
	tmpDir := t.TempDir()
	err := copyFile(filepath.Join(tmpDir, "missing"), filepath.Join(tmpDir, "dst"))
	if err == nil {
		t.Error("expected error when source doesn't exist")
	}
}

func TestCopyFileDestError(t *testing.T) {
	tmpDir := t.TempDir()
	src := filepath.Join(tmpDir, "src")
	_ = os.WriteFile(src, []byte("data"), 0644)

	// Dest path in nonexistent parent dir
	err := copyFile(src, filepath.Join(tmpDir, "subdir", "dst"))
	if err == nil {
		t.Error("expected error when dest dir doesn't exist")
	}
}

func TestSha256FileMissing(t *testing.T) {
	_, err := sha256File("/nonexistent/file")
	if err == nil {
		t.Error("expected error for missing file")
	}
}

func TestReadChecksumFileMissing(t *testing.T) {
	_, err := readChecksumFile("/nonexistent/checksum")
	if err == nil {
		t.Error("expected error for missing checksum file")
	}
}

func TestReadChecksumFileBlankLines(t *testing.T) {
	tmpDir := t.TempDir()
	f := filepath.Join(tmpDir, "checksums.sha256")
	_ = os.WriteFile(f, []byte("\n\n  \nabcdef1234567890  file.bin\n"), 0644)

	hash, err := readChecksumFile(f)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if hash != "abcdef1234567890" {
		t.Errorf("expected hash 'abcdef1234567890', got %q", hash)
	}
}

func TestVerifyChecksumSha256Error(t *testing.T) {
	tmpDir := t.TempDir()
	checksumFile := filepath.Join(tmpDir, "checksums.sha256")
	_ = os.WriteFile(checksumFile, []byte("abc123  file.bin\n"), 0644)

	err := VerifyChecksum(filepath.Join(tmpDir, "nonexistent"), checksumFile)
	if err == nil || !strings.Contains(err.Error(), "computing checksum") {
		t.Errorf("expected computing checksum error, got %v", err)
	}
}

func TestIsCrossDevice(t *testing.T) {
	// Test with a non-cross-device error
	regularErr := errors.New("regular error")
	if isCrossDevice(regularErr) {
		t.Error("expected false for regular error")
	}

	// Test with nil-ish cases
	if isCrossDevice(nil) {
		t.Error("expected false for nil error")
	}
}

func TestDownloadReadError(t *testing.T) {
	origTransport := http.DefaultTransport
	defer func() { http.DefaultTransport = origTransport }()

	http.DefaultTransport = &mockRoundTripper{
		roundTripFunc: func(req *http.Request) (*http.Response, error) {
			return &http.Response{
				StatusCode:    http.StatusOK,
				Body:          io.NopCloser(&errorReader{err: errors.New("read error"), afterBytes: 5}),
				ContentLength: 100,
			}, nil
		},
	}

	tmpDir := t.TempDir()
	dest := filepath.Join(tmpDir, "download-readfail")
	err := Download("http://example.com/binary", dest, func(downloaded, total int64) {})
	if err == nil || !strings.Contains(err.Error(), "reading response body") {
		t.Errorf("expected read error, got %v", err)
	}
}

type errorReader struct {
	err        error
	afterBytes int
	read       int
}

func (r *errorReader) Read(p []byte) (int, error) {
	if r.read >= r.afterBytes {
		return 0, r.err
	}
	n := len(p)
	remaining := r.afterBytes - r.read
	if n > remaining {
		n = remaining
	}
	for i := 0; i < n; i++ {
		p[i] = 'X'
	}
	r.read += n
	return n, nil
}

func TestLatestReleaseInvalidURL(t *testing.T) {
	// Test with invalid URL that causes http.NewRequest to fail
	_, err := LatestRelease("", "://invalid-url")
	if err == nil || !strings.Contains(err.Error(), "creating request") {
		t.Errorf("expected creating request error, got %v", err)
	}
}

func TestDownloadInvalidURL(t *testing.T) {
	tmpDir := t.TempDir()
	dest := filepath.Join(tmpDir, "download")
	err := Download("://invalid-url", dest, nil)
	if err == nil || !strings.Contains(err.Error(), "creating download request") {
		t.Errorf("expected creating download request error, got %v", err)
	}
}

func TestDownloadWriteError(t *testing.T) {
	origTransport := http.DefaultTransport
	defer func() { http.DefaultTransport = origTransport }()

	http.DefaultTransport = &mockRoundTripper{
		roundTripFunc: func(req *http.Request) (*http.Response, error) {
			return &http.Response{
				StatusCode:    http.StatusOK,
				Body:          io.NopCloser(strings.NewReader(strings.Repeat("X", 1024))),
				ContentLength: 1024,
			}, nil
		},
	}

	// /dev/full exists on Linux — writes to it fail with ENOSPC
	devFull := "/dev/full"
	if _, err := os.Stat(devFull); err == nil {
		err := Download("http://example.com/binary", devFull, func(downloaded, total int64) {})
		if err == nil {
			t.Error("expected write error to /dev/full")
		}
	}
}

func TestReadChecksumFileWhitespaceOnly(t *testing.T) {
	tmpDir := t.TempDir()
	f := filepath.Join(tmpDir, "checksums.sha256")
	_ = os.WriteFile(f, []byte("   \n\t\n  \n"), 0644)

	_, err := readChecksumFile(f)
	if err == nil || !strings.Contains(err.Error(), "empty checksum file") {
		t.Errorf("expected empty checksum file error, got %v", err)
	}
}
