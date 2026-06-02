package daemon

import (
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
)

// ---------------------------------------------------------------------------
// PIDFile — path construction
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// PIDFile — Write / Read cycle
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// PIDFile — Read when file does not exist
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// PIDFile — Read with malformed content
// ---------------------------------------------------------------------------

func TestPIDFile_Read_Malformed(t *testing.T) {
	tests := []struct {
		name    string
		content string
		wantErr bool
	}{
		{
			name:    "empty file",
			content: "",
			wantErr: true, // after TrimSpace, lines[0] is "" → Atoi fails
		},
		{
			name:    "non-numeric PID",
			content: "notapid\n2024-01-01T00:00:00Z\n",
			wantErr: true,
		},
		{
			name:    "pid only, no timestamp",
			content: "12345\n",
			wantErr: false,
		},
		{
			name:    "valid with bad timestamp",
			content: "12345\nnot-a-timestamp\n",
			wantErr: false, // timestamp parse error is silently ignored
		},
		{
			name:    "whitespace around PID",
			content: "  42  \n2024-01-01T00:00:00Z\n",
			wantErr: false,
		},
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

// ---------------------------------------------------------------------------
// PIDFile — Remove
// ---------------------------------------------------------------------------

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
	// Should not panic
	pf.Remove()
}

// ---------------------------------------------------------------------------
// PIDFile — IsAlive (test with current process PID)
// ---------------------------------------------------------------------------

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
	// Use a PID that is extremely unlikely to be alive.
	// PID 2^22 = 4194304 is valid on Linux but very unlikely to be running.
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
	// After IsAlive finds dead process, it should remove the pid file
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

// ---------------------------------------------------------------------------
// PIDFile — Signal edge case: no file
// ---------------------------------------------------------------------------

func TestPIDFile_Signal_NoFile(t *testing.T) {
	tmpDir := t.TempDir()
	pf := &PIDFile{path: filepath.Join(tmpDir, "nonexistent.pid")}
	err := pf.Signal(0)
	if err == nil {
		t.Error("expected error when no pid file exists")
	}
}

func TestPIDFile_SignalOS_NoFile(t *testing.T) {
	tmpDir := t.TempDir()
	pf := &PIDFile{path: filepath.Join(tmpDir, "nonexistent.pid")}
	err := pf.SignalOS(os.Kill)
	if err == nil {
		t.Error("expected error when no pid file exists")
	}
}

// ---------------------------------------------------------------------------
// PIDFile — Write creates directory
// ---------------------------------------------------------------------------

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

// splitTrimmed splits a string on newlines and returns non-empty trimmed lines.
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
