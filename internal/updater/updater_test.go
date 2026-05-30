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

	// 2. Checksum file
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
