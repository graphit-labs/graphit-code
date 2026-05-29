package slogutil

import (
	"context"
	"log/slog"
	"os"
)

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
