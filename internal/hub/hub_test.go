package hub

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestHubHashing(t *testing.T) {
	tempDir, err := os.MkdirTemp("", "graphit-hub-hash-test-*")
	if err != nil {
		t.Fatalf("failed to create temp dir: %v", err)
	}
	defer func() { _ = os.RemoveAll(tempDir) }()

	_, err = HashPath(filepath.Join(tempDir, "nonexistent"))
	if err == nil {
		t.Error("expected error for nonexistent path")
	}

	emptyDir := filepath.Join(tempDir, "empty")
	_ = os.Mkdir(emptyDir, 0755)
	_, err = HashPath(emptyDir)
	if err == nil || !strings.Contains(err.Error(), "contains no files") {
		t.Errorf("expected empty directory error, got: %v", err)
	}

	filePath := filepath.Join(tempDir, "file.txt")
	content := "some file content"
	_ = os.WriteFile(filePath, []byte(content), 0644)

	h1, err := HashFile(filePath)
	if err != nil {
		t.Fatalf("failed to hash file: %v", err)
	}

	h2, err := HashPath(filePath)
	if err != nil {
		t.Fatalf("failed to hash path (file): %v", err)
	}

	if h1 != h2 {
		t.Errorf("expected same hash from HashFile and HashPath: %s vs %s", h1, h2)
	}

	dirPath := filepath.Join(tempDir, "dir")
	_ = os.Mkdir(dirPath, 0755)
	_ = os.WriteFile(filepath.Join(dirPath, "f1.txt"), []byte("c1"), 0644)
	_ = os.WriteFile(filepath.Join(dirPath, "f2.txt"), []byte("c2"), 0644)

	hDir, err := HashDirectory(dirPath)
	if err != nil {
		t.Fatalf("failed to hash directory: %v", err)
	}
	if hDir == "" {
		t.Error("expected non-empty directory hash")
	}

	truncated := TruncateHash(hDir, 8)
	if len(truncated) != 8 {
		t.Errorf("expected length 8, got %d", len(truncated))
	}
	truncatedShort := TruncateHash("123", 8)
	if truncatedShort != "123" {
		t.Errorf("expected '123', got %q", truncatedShort)
	}

	ok, err := VerifyHash(filePath, h1)
	if err != nil || !ok {
		t.Errorf("VerifyHash failed: %v, %t", err, ok)
	}

	okEmpty, err := VerifyHash(filePath, "")
	if err != nil || !okEmpty {
		t.Errorf("VerifyHash with empty expected should succeed: %v, %t", err, okEmpty)
	}

	okFail, _ := VerifyHash(filePath, "wronghash")
	if okFail {
		t.Error("VerifyHash with wrong hash should fail")
	}
}
