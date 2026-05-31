package updater

import (
	"bufio"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"runtime"
	"strings"
	"time"
)

const (
	githubAPIBase = "https://api.github.com"
	userAgent     = "graphit-self-update/1"
	httpTimeout   = 60 * time.Second
)

type Release struct {
	TagName string  `json:"tag_name"`
	Assets  []Asset `json:"assets"`
}

type Asset struct {
	Name               string `json:"name"`
	BrowserDownloadURL string `json:"browser_download_url"`
	Size               int64  `json:"size"`
}

func LatestRelease(repo, selfUpdateURL string) (*Release, error) {
	var url string
	if selfUpdateURL != "" {
		url = strings.TrimRight(selfUpdateURL, "/")
	} else {
		url = fmt.Sprintf("%s/repos/%s/releases/latest", githubAPIBase, repo)
	}

	req, err := http.NewRequest(http.MethodGet, url, nil)
	if err != nil {
		return nil, fmt.Errorf("creating request: %w", err)
	}
	req.Header.Set("Accept", "application/vnd.github+json")
	req.Header.Set("User-Agent", userAgent)
	req.Header.Set("X-GitHub-Api-Version", "2022-11-28")

	client := &http.Client{Timeout: httpTimeout}
	resp, err := client.Do(req)
	if err != nil {
		return nil, fmt.Errorf("fetching latest release: %w", err)
	}
	defer func() { _ = resp.Body.Close() }()

	if resp.StatusCode == http.StatusNotFound {
		return nil, fmt.Errorf("no releases found for repository %q", repo)
	}
	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("GitHub API returned status %d", resp.StatusCode)
	}

	var release Release
	if err := json.NewDecoder(resp.Body).Decode(&release); err != nil {
		return nil, fmt.Errorf("decoding response: %w", err)
	}
	return &release, nil
}

func PlatformBinaryName(binName string) string {
	os := runtime.GOOS
	arch := runtime.GOARCH
	if os == "windows" {
		return fmt.Sprintf("%s-%s-%s.exe", binName, os, arch)
	}
	return fmt.Sprintf("%s-%s-%s", binName, os, arch)
}

func FindAsset(release *Release, assetName string) string {
	for _, a := range release.Assets {
		if a.Name == assetName {
			return a.BrowserDownloadURL
		}
	}
	return ""
}

func NeedsUpdate(currentVersion, latestVersion string) bool {
	if currentVersion == "dev" {
		return false
	}
	cur := strings.TrimPrefix(currentVersion, "v")
	lat := strings.TrimPrefix(latestVersion, "v")
	return lat != cur && compareVersions(lat, cur) > 0
}

func Download(url, destPath string, progressFn func(downloaded, total int64)) error {
	req, err := http.NewRequest(http.MethodGet, url, nil)
	if err != nil {
		return fmt.Errorf("creating download request: %w", err)
	}
	req.Header.Set("User-Agent", userAgent)

	client := &http.Client{Timeout: 10 * time.Minute}
	resp, err := client.Do(req)
	if err != nil {
		return fmt.Errorf("downloading %s: %w", url, err)
	}
	defer func() { _ = resp.Body.Close() }()

	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("server returned status %d for %s", resp.StatusCode, url)
	}

	f, err := os.OpenFile(destPath, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, 0o755)
	if err != nil {
		return fmt.Errorf("creating destination file: %w", err)
	}
	defer func() { _ = f.Close() }()

	if progressFn == nil {
		_, err = io.Copy(f, resp.Body)
		return err
	}

	var downloaded int64
	buf := make([]byte, 32*1024)
	for {
		n, readErr := resp.Body.Read(buf)
		if n > 0 {
			_, writeErr := f.Write(buf[:n])
			if writeErr != nil {
				return fmt.Errorf("writing to file: %w", writeErr)
			}
			downloaded += int64(n)
			progressFn(downloaded, resp.ContentLength)
		}
		if readErr == io.EOF {
			break
		}
		if readErr != nil {
			return fmt.Errorf("reading response body: %w", readErr)
		}
	}
	return nil
}

func VerifyChecksum(filePath, checksumFilePath string) error {
	expectedHash, err := readChecksumFile(checksumFilePath)
	if err != nil {
		return fmt.Errorf("reading checksum file: %w", err)
	}

	actualHash, err := sha256File(filePath)
	if err != nil {
		return fmt.Errorf("computing checksum: %w", err)
	}

	if !strings.EqualFold(actualHash, expectedHash) {
		return fmt.Errorf("checksum mismatch: expected %s, got %s", expectedHash, actualHash)
	}
	return nil
}

func readChecksumFile(path string) (string, error) {
	f, err := os.Open(path)
	if err != nil {
		return "", err
	}
	defer func() { _ = f.Close() }()

	scanner := bufio.NewScanner(f)
	for scanner.Scan() {
		line := strings.TrimSpace(scanner.Text())
		if line == "" {
			continue
		}
		parts := strings.Fields(line)
		return parts[0], nil
	}
	return "", fmt.Errorf("empty checksum file: %s", path)
}

func AtomicReplace(newBinary, currentExe string) error {
	backupPath := currentExe + ".bak"
	_ = os.Remove(backupPath)

	if err := os.Rename(currentExe, backupPath); err != nil {
		return fmt.Errorf("backing up current binary: %w", err)
	}

	if err := os.Rename(newBinary, currentExe); err != nil {
		if isCrossDevice(err) {
			if cpErr := copyFile(newBinary, currentExe); cpErr != nil {
				_ = os.Rename(backupPath, currentExe)
				return fmt.Errorf("copying binary across filesystems: %w", cpErr)
			}
			_ = os.Remove(newBinary)
		} else {
			_ = os.Rename(backupPath, currentExe)
			return fmt.Errorf("replacing binary: %w", err)
		}
	}

	_ = os.Remove(backupPath)
	return nil
}

func copyFile(src, dst string) error {
	in, err := os.Open(src)
	if err != nil {
		return err
	}
	defer func() { _ = in.Close() }()

	info, err := in.Stat()
	if err != nil {
		return err
	}

	out, err := os.OpenFile(dst, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, info.Mode())
	if err != nil {
		return err
	}
	defer func() { _ = out.Close() }()

	if _, err := io.Copy(out, in); err != nil {
		return err
	}
	return out.Close()
}


func sha256File(path string) (string, error) {
	f, err := os.Open(path)
	if err != nil {
		return "", err
	}
	defer func() { _ = f.Close() }()

	h := sha256.New()
	if _, err := io.Copy(h, f); err != nil {
		return "", err
	}
	return hex.EncodeToString(h.Sum(nil)), nil
}



func compareVersions(a, b string) int {
	aParts := strings.SplitN(a, "-", 2)
	bParts := strings.SplitN(b, "-", 2)

	aSegs := strings.Split(aParts[0], ".")
	bSegs := strings.Split(bParts[0], ".")

	maxLen := len(aSegs)
	if len(bSegs) > maxLen {
		maxLen = len(bSegs)
	}

	for i := 0; i < maxLen; i++ {
		var av, bv int
		if i < len(aSegs) {
			_, _ = fmt.Sscanf(aSegs[i], "%d", &av)
		}
		if i < len(bSegs) {
			_, _ = fmt.Sscanf(bSegs[i], "%d", &bv)
		}
		if av > bv {
			return 1
		}
		if av < bv {
			return -1
		}
	}

	aPre := ""
	if len(aParts) > 1 {
		aPre = aParts[1]
	}
	bPre := ""
	if len(bParts) > 1 {
		bPre = bParts[1]
	}

	if aPre == "" && bPre != "" {
		return 1
	}
	if aPre != "" && bPre == "" {
		return -1
	}
	if aPre < bPre {
		return -1
	}
	if aPre > bPre {
		return 1
	}
	return 0
}
