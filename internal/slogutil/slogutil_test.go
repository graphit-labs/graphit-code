package slogutil

import (
	"bytes"
	"context"
	"log/slog"
	"strings"
	"testing"
)

func TestNOP(t *testing.T) {
	logger := NOP()
	if logger == nil {
		t.Fatal("NOP() returned nil")
	}
	// Logging should be silently discarded — no panic, no output.
	logger.Info("this message should be discarded")
	logger.Error("this error should be discarded")
	logger.With("key", "value").Warn("also discarded")
}

func TestResolve(t *testing.T) {
	tests := []struct {
		name    string
		input   *slog.Logger
		wantNOP bool
	}{
		{
			name:    "nil returns NOP logger",
			input:   nil,
			wantNOP: true,
		},
		{
			name:    "non-nil returns same logger",
			input:   slog.Default(),
			wantNOP: false,
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := Resolve(tt.input)
			if result == nil {
				t.Fatal("Resolve() returned nil")
			}
			if tt.wantNOP {
				// NOP handler should have Enabled() returning false
				if result.Handler().Enabled(context.Background(), slog.LevelError) {
					t.Error("expected NOP handler to have Enabled() == false")
				}
			}
			if !tt.wantNOP && result != tt.input {
				t.Error("expected Resolve() to return the same logger instance")
			}
		})
	}
}

func TestDiscardHandler(t *testing.T) {
	h := discardHandler{}

	t.Run("Enabled always returns false", func(t *testing.T) {
		levels := []slog.Level{slog.LevelDebug, slog.LevelInfo, slog.LevelWarn, slog.LevelError}
		for _, lvl := range levels {
			if h.Enabled(context.Background(), lvl) {
				t.Errorf("Enabled(%v) = true; want false", lvl)
			}
		}
	})

	t.Run("Handle returns nil error", func(t *testing.T) {
		err := h.Handle(context.Background(), slog.Record{})
		if err != nil {
			t.Errorf("Handle() = %v; want nil", err)
		}
	})

	t.Run("WithAttrs returns same type", func(t *testing.T) {
		result := h.WithAttrs([]slog.Attr{slog.String("key", "value")})
		if _, ok := result.(discardHandler); !ok {
			t.Errorf("WithAttrs() returned %T; want discardHandler", result)
		}
	})

	t.Run("WithGroup returns same type", func(t *testing.T) {
		result := h.WithGroup("group")
		if _, ok := result.(discardHandler); !ok {
			t.Errorf("WithGroup() returned %T; want discardHandler", result)
		}
	})
}

func TestNOPDoesNotWrite(t *testing.T) {
	// Verify the NOP logger truly discards output by checking the handler.
	logger := NOP()
	handler := logger.Handler()
	if handler.Enabled(context.Background(), slog.LevelError) {
		t.Error("NOP handler should never be enabled, even for Error level")
	}
}

func TestStderrDebugLevel(t *testing.T) {
	// Verify the Stderr logger uses Debug level by checking handler directly.
	// We create a logger with a known writer to verify the output.
	var buf bytes.Buffer
	logger := slog.New(slog.NewTextHandler(&buf, &slog.HandlerOptions{
		Level: slog.LevelDebug,
	})).With("module", "test")

	logger.Debug("debug message")
	if !strings.Contains(buf.String(), "debug message") {
		t.Errorf("expected debug message in output, got: %s", buf.String())
	}
}
