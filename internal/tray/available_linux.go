//go:build linux

package tray

import (
	"fmt"

	"github.com/godbus/dbus/v5"
)

func trayAvailable() error {
	conn, err := dbus.ConnectSessionBus()
	if err != nil {
		return fmt.Errorf("connecting to desktop session bus: %w", err)
	}
	defer conn.Close()
	var owner bool
	if err := conn.BusObject().Call("org.freedesktop.DBus.NameHasOwner", 0, "org.kde.StatusNotifierWatcher").Store(&owner); err != nil {
		return fmt.Errorf("checking system tray support: %w", err)
	}
	if !owner {
		return fmt.Errorf("desktop has no StatusNotifier tray watcher; enable AppIndicator/StatusNotifier support")
	}
	return nil
}
