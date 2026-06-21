package daemon

import (
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"syscall"
	"testing"
)

func TestNewPIDFile_Path(t *testing.T) {
	tempHome := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tempHome)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	pf := NewPIDFile()
	expected := filepath.Join(GlobalDaemonDir(), pidFileName)
	if pf.Path() != expected {
		t.Errorf("expected path %q, got %q", expected, pf.Path())
	}
}

func TestPIDFile_WriteAndRead(t *testing.T) {
	tempHome := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tempHome)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	pf := NewPIDFile()
	if err := pf.Write(); err != nil {
		t.Fatalf("Write: %v", err)
	}

	pd, err := pf.Read()
	if err != nil {
		t.Fatalf("Read: %v", err)
	}
	if pd == nil {
		t.Fatal("Read returned nil pidData")
	}
	if pd.PID != os.Getpid() {
		t.Errorf("expected PID %d, got %d", os.Getpid(), pd.PID)
	}
	if pd.StartedAt.IsZero() {
		t.Error("StartedAt should not be zero")
	}
}

func TestPIDFile_Write_MkdirError(t *testing.T) {
	tmpDir := t.TempDir()
	blockFile := filepath.Join(tmpDir, "block")
	_ = os.WriteFile(blockFile, []byte("x"), 0o600)
	pf := &PIDFile{path: filepath.Join(blockFile, "sub", "test.pid")}
	err := pf.Write()
	if err == nil {
		t.Error("expected error when dir creation fails")
	}
	if !strings.Contains(err.Error(), "creating pid dir") {
		t.Errorf("expected 'creating pid dir' in error, got %v", err)
	}
}

func TestPIDFile_Read_NoFile(t *testing.T) {
	tmpDir := t.TempDir()
	pf := &PIDFile{path: filepath.Join(tmpDir, "nonexistent.pid")}

	pd, err := pf.Read()
	if err != nil {
		t.Errorf("expected no error for missing file, got %v", err)
	}
	if pd != nil {
		t.Errorf("expected nil pidData, got %+v", pd)
	}
}

func TestPIDFile_Read_Malformed(t *testing.T) {
	tests := []struct {
		name    string
		content string
		wantErr bool
	}{
		{"empty file", "", true},
		{"non-numeric PID", "notapid\n2024-01-01T00:00:00Z\n", true},
		{"pid only, no timestamp", "12345\n", false},
		{"valid with bad timestamp", "12345\nnot-a-timestamp\n", false},
		{"whitespace around PID", "  42  \n2024-01-01T00:00:00Z\n", false},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			tmpDir := t.TempDir()
			path := filepath.Join(tmpDir, "test.pid")
			if err := os.WriteFile(path, []byte(tc.content), 0o600); err != nil {
				t.Fatalf("write: %v", err)
			}
			pf := &PIDFile{path: path}
			pd, err := pf.Read()
			if tc.wantErr {
				if err == nil {
					t.Errorf("expected error, got nil (pd=%+v)", pd)
				}
			} else {
				if err != nil {
					t.Errorf("unexpected error: %v", err)
				}
				if pd == nil {
					t.Fatal("expected non-nil pidData")
				}
			}
		})
	}
}

func TestPIDFile_Read_PIDOnlyNoTimestamp(t *testing.T) {
	tmpDir := t.TempDir()
	path := filepath.Join(tmpDir, "test.pid")
	if err := os.WriteFile(path, []byte("9999\n"), 0o600); err != nil {
		t.Fatalf("write: %v", err)
	}
	pf := &PIDFile{path: path}
	pd, err := pf.Read()
	if err != nil {
		t.Fatalf("Read: %v", err)
	}
	if pd.PID != 9999 {
		t.Errorf("expected PID 9999, got %d", pd.PID)
	}
	if !pd.StartedAt.IsZero() {
		t.Errorf("expected zero StartedAt when only PID line exists, got %v", pd.StartedAt)
	}
}

func TestPIDFile_Remove(t *testing.T) {
	tmpDir := t.TempDir()
	path := filepath.Join(tmpDir, "test.pid")
	if err := os.WriteFile(path, []byte("123\n"), 0o600); err != nil {
		t.Fatalf("write: %v", err)
	}
	pf := &PIDFile{path: path}
	pf.Remove()

	if _, err := os.Stat(path); !os.IsNotExist(err) {
		t.Errorf("expected file to be removed, stat err: %v", err)
	}
}

func TestPIDFile_Remove_NoFile(t *testing.T) {
	tmpDir := t.TempDir()
	pf := &PIDFile{path: filepath.Join(tmpDir, "nonexistent.pid")}
	pf.Remove()
}

func TestPIDFile_IsAlive_CurrentProcess(t *testing.T) {
	tmpDir := t.TempDir()
	path := filepath.Join(tmpDir, "test.pid")
	content := fmt.Sprintf("%d\n2024-01-01T00:00:00Z\n", os.Getpid())
	if err := os.WriteFile(path, []byte(content), 0o600); err != nil {
		t.Fatalf("write: %v", err)
	}
	pf := &PIDFile{path: path}
	pd := pf.IsAlive()
	if pd == nil {
		t.Error("IsAlive returned nil for current process PID — expected alive")
	} else if pd.PID != os.Getpid() {
		t.Errorf("expected PID %d, got %d", os.Getpid(), pd.PID)
	}
}

func TestPIDFile_IsAlive_DeadProcess(t *testing.T) {
	tmpDir := t.TempDir()
	path := filepath.Join(tmpDir, "test.pid")
	fakePID := 4194304
	content := fmt.Sprintf("%d\n2024-01-01T00:00:00Z\n", fakePID)
	if err := os.WriteFile(path, []byte(content), 0o600); err != nil {
		t.Fatalf("write: %v", err)
	}
	pf := &PIDFile{path: path}
	pd := pf.IsAlive()
	if pd != nil {
		t.Errorf("IsAlive returned non-nil for dead PID %d", fakePID)
	}
	if _, err := os.Stat(path); !os.IsNotExist(err) {
		t.Error("IsAlive should remove stale pid file for dead process")
	}
}

func TestPIDFile_IsAlive_NoFile(t *testing.T) {
	tmpDir := t.TempDir()
	pf := &PIDFile{path: filepath.Join(tmpDir, "nonexistent.pid")}
	if pd := pf.IsAlive(); pd != nil {
		t.Errorf("expected nil for missing file, got %+v", pd)
	}
}

func TestPIDFile_IsAlive_MalformedFile(t *testing.T) {
	tmpDir := t.TempDir()
	path := filepath.Join(tmpDir, "test.pid")
	if err := os.WriteFile(path, []byte("notapid\n"), 0o600); err != nil {
		t.Fatalf("write: %v", err)
	}
	pf := &PIDFile{path: path}
	pd := pf.IsAlive()
	if pd != nil {
		t.Errorf("expected nil for malformed pid file, got %+v", pd)
	}
}

func TestPIDFile_Signal_NoFile(t *testing.T) {
	tmpDir := t.TempDir()
	pf := &PIDFile{path: filepath.Join(tmpDir, "nonexistent.pid")}
	err := pf.Signal(syscall.Signal(0))
	if err == nil {
		t.Error("expected error when no pid file exists")
	}
}

func TestPIDFile_Signal_CurrentProcess(t *testing.T) {
	tmpDir := t.TempDir()
	path := filepath.Join(tmpDir, "test.pid")
	content := fmt.Sprintf("%d\n2024-01-01T00:00:00Z\n", os.Getpid())
	if err := os.WriteFile(path, []byte(content), 0o600); err != nil {
		t.Fatalf("write: %v", err)
	}
	pf := &PIDFile{path: path}
	err := pf.Signal(syscall.Signal(0))
	if err != nil {
		t.Errorf("expected no error for signal 0 to current process, got %v", err)
	}
}

func TestPIDFile_Signal_DeadProcess(t *testing.T) {
	tmpDir := t.TempDir()
	path := filepath.Join(tmpDir, "test.pid")
	fakePID := 4194304
	content := fmt.Sprintf("%d\n2024-01-01T00:00:00Z\n", fakePID)
	if err := os.WriteFile(path, []byte(content), 0o600); err != nil {
		t.Fatalf("write: %v", err)
	}
	pf := &PIDFile{path: path}
	err := pf.Signal(syscall.Signal(0))
	if err == nil {
		t.Error("expected error signalling dead process")
	}
}

func TestPIDFile_Write_CreatesDir(t *testing.T) {
	tmpDir := t.TempDir()
	path := filepath.Join(tmpDir, "deep", "nested", "dir", "test.pid")
	pf := &PIDFile{path: path}
	if err := pf.Write(); err != nil {
		t.Fatalf("Write: %v", err)
	}
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("Read: %v", err)
	}
	lines := splitTrimmed(string(data))
	if len(lines) < 1 {
		t.Fatal("expected at least 1 line")
	}
	pid, err := strconv.Atoi(lines[0])
	if err != nil {
		t.Fatalf("parse PID: %v", err)
	}
	if pid != os.Getpid() {
		t.Errorf("expected PID %d, got %d", os.Getpid(), pid)
	}
}

func splitTrimmed(s string) []string {
	var result []string
	for _, l := range strings.Split(strings.TrimSpace(s), "\n") {
		l = strings.TrimSpace(l)
		if l != "" {
			result = append(result, l)
		}
	}
	return result
}
