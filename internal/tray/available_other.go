//go:build !linux

package tray

func trayAvailable() error { return nil }
