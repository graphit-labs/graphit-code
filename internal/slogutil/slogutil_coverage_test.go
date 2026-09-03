package slogutil

import (
	"os"
	"path/filepath"
	"testing"
)

func resetLogFileState(t *testing.T) {
	t.Helper()
	logFileMu.Lock()
	defer logFileMu.Unlock()
	if logFile != nil {
		_ = logFile.Close()
		logFile = nil
	}
}

func TestInitFileLogger_EmptyDir(t *testing.T) {
	resetLogFileState(t)
	defer resetLogFileState(t)

	InitFileLogger("")
	logFileMu.Lock()
	lf := logFile
	logFileMu.Unlock()
	if lf != nil {
		t.Fatal("expected logFile to be nil when globalDir is empty")
	}
}

func TestInitFileLogger_CreatesLogFile(t *testing.T) {
	resetLogFileState(t)
	defer resetLogFileState(t)

	dir := t.TempDir()
	InitFileLogger(dir)

	logPath := filepath.Join(dir, "logs", "graphit.log")
	info, err := os.Stat(logPath)
	if err != nil {
		t.Fatalf("log file not created: %v", err)
	}
	if info.IsDir() {
		t.Fatal("expected a file, got a directory")
	}

	logFileMu.Lock()
	lf := logFile
	logFileMu.Unlock()
	if lf == nil {
		t.Fatal("expected logFile to be set after InitFileLogger")
	}
}

func TestInitFileLogger_TruncatesLargeFile(t *testing.T) {
	resetLogFileState(t)
	defer resetLogFileState(t)

	dir := t.TempDir()
	logDir := filepath.Join(dir, "logs")
	if err := os.MkdirAll(logDir, 0o755); err != nil {
		t.Fatal(err)
	}

	logPath := filepath.Join(logDir, "graphit.log")
	f, err := os.Create(logPath)
	if err != nil {
		t.Fatal(err)
	}
	largeData := make([]byte, 6*1024*1024)
	if _, err := f.Write(largeData); err != nil {
		f.Close()
		t.Fatal(err)
	}
	f.Close()

	info, _ := os.Stat(logPath)
	if info.Size() <= 5*1024*1024 {
		t.Fatal("pre-condition failed: file should be >5MB")
	}

	InitFileLogger(dir)

	info, err = os.Stat(logPath)
	if err != nil {
		t.Fatalf("log file missing after truncation: %v", err)
	}
	if info.Size() > 5*1024*1024 {
		t.Errorf("expected file to be truncated, got size %d", info.Size())
	}
}

func TestInitFileLogger_SmallFileNotTruncated(t *testing.T) {
	resetLogFileState(t)
	defer resetLogFileState(t)

	dir := t.TempDir()
	logDir := filepath.Join(dir, "logs")
	if err := os.MkdirAll(logDir, 0o755); err != nil {
		t.Fatal(err)
	}

	logPath := filepath.Join(logDir, "graphit.log")
	content := []byte("small log content\n")
	if err := os.WriteFile(logPath, content, 0o644); err != nil {
		t.Fatal(err)
	}

	InitFileLogger(dir)

	info, err := os.Stat(logPath)
	if err != nil {
		t.Fatal(err)
	}
	if info.Size() < int64(len(content)) {
		t.Errorf("expected file to retain content, got size %d", info.Size())
	}
}

func TestInitFileLogger_MkdirAllFails(t *testing.T) {
	resetLogFileState(t)
	defer resetLogFileState(t)

	dir := t.TempDir()
	blocker := filepath.Join(dir, "logs")
	if err := os.WriteFile(blocker, []byte("block"), 0o644); err != nil {
		t.Fatal(err)
	}

	InitFileLogger(dir)

	logFileMu.Lock()
	lf := logFile
	logFileMu.Unlock()
	if lf != nil {
		t.Fatal("expected logFile to be nil when MkdirAll fails")
	}
}

func TestInitFileLogger_OpenFileFails(t *testing.T) {
	resetLogFileState(t)
	defer resetLogFileState(t)

	dir := t.TempDir()
	logDir := filepath.Join(dir, "logs")
	if err := os.MkdirAll(logDir, 0o755); err != nil {
		t.Fatal(err)
	}

	logPath := filepath.Join(logDir, "graphit.log")
	if err := os.MkdirAll(logPath, 0o755); err != nil {
		t.Fatal(err)
	}

	InitFileLogger(dir)

	logFileMu.Lock()
	lf := logFile
	logFileMu.Unlock()
	if lf != nil {
		t.Fatal("expected logFile to be nil when OpenFile fails")
	}
}

func TestCloseFileLogger_Idempotent(t *testing.T) {
	resetLogFileState(t)

	dir := t.TempDir()
	InitFileLogger(dir)

	CloseFileLogger()
	logFileMu.Lock()
	lf := logFile
	logFileMu.Unlock()
	if lf != nil {
		t.Fatal("expected logFile to be nil after CloseFileLogger")
	}

	CloseFileLogger()
}

func TestCloseFileLogger_WithoutInit(t *testing.T) {
	resetLogFileState(t)

	CloseFileLogger()
}
