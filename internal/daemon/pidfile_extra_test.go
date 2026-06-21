package daemon

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"syscall"
	"testing"
)

func TestPIDFile_Read_PermissionError(t *testing.T) {
	tmpDir := t.TempDir()
	path := filepath.Join(tmpDir, "test.pid")
	if err := os.WriteFile(path, []byte("123\n"), 0o600); err != nil {
		t.Fatalf("write: %v", err)
	}
	if err := os.Chmod(path, 0o000); err != nil {
		t.Fatalf("chmod: %v", err)
	}
	defer func() { _ = os.Chmod(path, 0o600) }()

	pf := &PIDFile{path: path}
	_, err := pf.Read()
	if err == nil {
		t.Error("expected error when file is not readable")
	}
}

func TestPIDFile_Read_EmptyContent(t *testing.T) {
	t.Parallel()
	tmpDir := t.TempDir()
	path := filepath.Join(tmpDir, "test.pid")
	_ = os.WriteFile(path, []byte(""), 0o600)
	pf := &PIDFile{path: path}
	_, err := pf.Read()
	if err == nil {
		t.Error("expected error for empty file content")
	}
	if !strings.Contains(err.Error(), "invalid") && !strings.Contains(err.Error(), "malformed") {
		t.Errorf("expected 'invalid' or 'malformed' in error, got %v", err)
	}
}

func TestPIDFile_IsAlive_ReadError(t *testing.T) {
	tmpDir := t.TempDir()
	path := filepath.Join(tmpDir, "test.pid")
	_ = os.WriteFile(path, []byte("not-a-number\n"), 0o600)
	pf := &PIDFile{path: path}
	pd := pf.IsAlive()
	if pd != nil {
		t.Error("expected nil when Read returns error")
	}
}

func TestPIDFile_Signal_MalformedFile(t *testing.T) {
	tmpDir := t.TempDir()
	path := filepath.Join(tmpDir, "test.pid")
	_ = os.WriteFile(path, []byte("bad\n"), 0o600)
	pf := &PIDFile{path: path}
	err := pf.Signal(syscall.Signal(0))
	if err == nil {
		t.Error("expected error for malformed pid file")
	}
}

func TestPIDFile_Path(t *testing.T) {
	t.Parallel()
	pf := &PIDFile{path: "/some/path/test.pid"}
	if pf.Path() != "/some/path/test.pid" {
		t.Errorf("expected '/some/path/test.pid', got %q", pf.Path())
	}
}

func TestPIDFile_WriteRead_TimestampParsing(t *testing.T) {
	tmpDir := t.TempDir()
	path := filepath.Join(tmpDir, "test.pid")
	pf := &PIDFile{path: path}

	if err := pf.Write(); err != nil {
		t.Fatalf("Write: %v", err)
	}

	pd, err := pf.Read()
	if err != nil {
		t.Fatalf("Read: %v", err)
	}
	if pd == nil {
		t.Fatal("expected non-nil pidData")
	}
	if pd.PID != os.Getpid() {
		t.Errorf("PID: expected %d, got %d", os.Getpid(), pd.PID)
	}
	if pd.StartedAt.IsZero() {
		t.Error("StartedAt should not be zero")
	}
}

func TestPIDFile_Signal_OwnProcess(t *testing.T) {
	tmpDir := t.TempDir()
	path := filepath.Join(tmpDir, "test.pid")
	content := fmt.Sprintf("%d\n2024-01-01T00:00:00Z\n", os.Getpid())
	_ = os.WriteFile(path, []byte(content), 0o600)
	pf := &PIDFile{path: path}

	err := pf.Signal(syscall.Signal(0))
	if err != nil {
		t.Errorf("expected no error signalling own process, got %v", err)
	}
}
