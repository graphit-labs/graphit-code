package slogutil

import (
	"context"
	"io"
	"log/slog"
	"os"
	"path/filepath"
	"sync"
)

// logFile holds the currently open log file so it can be closed on shutdown.
var (
	logFile   *os.File
	logFileMu sync.Mutex
)

// InitFileLogger configures the global slog default to write structured logs
// to both stderr and a persistent log file at <globalDir>/logs/graphit.log.
// This ensures logs are always accessible regardless of how the process runs
// (daemon, MCP stdio, CLI).
//
// If the log file cannot be opened (e.g. permission error), logging falls
// back to stderr only — it never prevents the process from starting.
func InitFileLogger(globalDir string) {
	if globalDir == "" {
		return
	}

	logDir := filepath.Join(globalDir, "logs")
	if err := os.MkdirAll(logDir, 0o755); err != nil {
		return
	}

	logPath := filepath.Join(logDir, "graphit.log")

	// Truncate if the log file is too large (>5 MB) to prevent unbounded growth.
	if info, err := os.Stat(logPath); err == nil && info.Size() > 5*1024*1024 {
		_ = os.Truncate(logPath, 0)
	}

	f, err := os.OpenFile(logPath, os.O_CREATE|os.O_WRONLY|os.O_APPEND, 0o644)
	if err != nil {
		return
	}

	logFileMu.Lock()
	logFile = f
	logFileMu.Unlock()

	w := io.MultiWriter(os.Stderr, f)
	handler := slog.NewTextHandler(w, &slog.HandlerOptions{
		Level: slog.LevelInfo,
	})
	slog.SetDefault(slog.New(handler))
}

// CloseFileLogger closes the log file if it was opened. Safe to call multiple times.
func CloseFileLogger() {
	logFileMu.Lock()
	defer logFileMu.Unlock()
	if logFile != nil {
		_ = logFile.Close()
		logFile = nil
	}
}

func NOP() *slog.Logger {
	return slog.New(discardHandler{})
}

func Stderr(module string) *slog.Logger {
	return slog.New(slog.NewTextHandler(os.Stderr, &slog.HandlerOptions{
		Level: slog.LevelDebug,
	})).With("module", module)
}

func Resolve(l *slog.Logger) *slog.Logger {
	if l != nil {
		return l
	}
	return NOP()
}

type discardHandler struct{}

func (discardHandler) Enabled(context.Context, slog.Level) bool  { return false }
func (discardHandler) Handle(context.Context, slog.Record) error { return nil }
func (d discardHandler) WithAttrs([]slog.Attr) slog.Handler      { return d }
func (d discardHandler) WithGroup(string) slog.Handler            { return d }

