// Package tray runs the Graphit status icon in a graphical user session.
// It is separate from the headless daemon service.
package tray

import (
	"context"
	_ "embed"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"os/signal"
	"path/filepath"
	"runtime"
	"syscall"
	"time"

	"github.com/gogpu/systray"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/daemonctl"
	"github.com/graphit-labs/graphit-code/internal/lockfile"
	"github.com/graphit-labs/graphit-code/internal/sysutil"
)

//go:embed assets/icon.png
var iconPNG []byte

//go:embed assets/icon_template.png
var templatePNG []byte

func trayLockPath() string { return filepath.Join(daemonctl.DaemonDir(), "tray.lock") }

func graphicalSession() bool {
	if runtime.GOOS != "linux" {
		return true
	}
	return (os.Getenv("DISPLAY") != "" || os.Getenv("WAYLAND_DISPLAY") != "") && os.Getenv("DBUS_SESSION_BUS_ADDRESS") != ""
}

// EnsureRunning is best-effort session UI startup. It never starts the daemon.
func EnsureRunning() error {
	if !graphicalSession() {
		return nil
	}
	if err := trayAvailable(); err != nil {
		return err
	}
	if err := os.MkdirAll(daemonctl.DaemonDir(), 0o700); err != nil {
		return err
	}
	spawn, err := lockfile.Acquire(filepath.Join(daemonctl.DaemonDir(), ".tray-spawn.lock"), time.Second)
	if err != nil {
		return err
	}
	defer spawn.Release()
	if locked, err := lockfile.IsLocked(trayLockPath()); err != nil {
		return err
	} else if locked {
		return nil
	}
	exe := daemonctl.ResolveExe()
	if exe == "" {
		return fmt.Errorf("cannot resolve Graphit launcher for tray")
	}
	cmd := exec.Command(exe, "tray")
	cmd.Stdin = nil
	cmd.Stdout = nil
	closeLog := daemonctl.AttachStderrToFile(cmd, filepath.Join(daemonctl.DaemonDir(), "tray.log"))
	defer closeLog()
	sysutil.DetachProcess(cmd)
	if err := cmd.Start(); err != nil {
		return err
	}
	go func() { _ = cmd.Wait() }()
	return nil
}

// Run owns the UI event loop and one tray icon for the current user session.
func Run() error {
	if !graphicalSession() {
		return fmt.Errorf("no graphical session with a D-Bus session bus is available")
	}
	if err := trayAvailable(); err != nil {
		return err
	}
	lock, err := lockfile.TryAcquire(trayLockPath())
	if errors.Is(err, lockfile.ErrLocked) {
		return nil
	}
	if err != nil {
		return err
	}
	defer lock.Release()

	ui := systray.New()
	menu := systray.NewMenu()
	stateItem := menu.Add("Checking daemon…", nil)
	stateItem.SetDisabled(true)
	activityItem := menu.Add("Checking activity…", nil)
	activityItem.SetDisabled(true)
	menu.AddSeparator()
	menu.Add("Open UI", func() {
		go func() {
			if err := OpenUI(); err != nil {
				ui.ShowNotification(brand.DisplayName, "UI unavailable: "+short(err.Error(), 100))
			}
		}()
	})
	startItem := menu.Add("Start daemon", func() {
		go func() {
			if _, err := daemonctl.EnsureRunning(); err != nil {
				ui.ShowNotification(brand.DisplayName, "Start failed: "+short(err.Error(), 100))
			}
		}()
	})
	stopItem := menu.Add("Stop daemon", func() {
		go func() {
			if err := daemonctl.Stop(); err != nil {
				ui.ShowNotification(brand.DisplayName, "Stop failed: "+short(err.Error(), 100))
			}
		}()
	})
	menu.AddSeparator()
	menu.Add("Stop daemon and quit", func() {
		go func() {
			if err := stopAndQuit(daemonctl.Stop, ui.Remove); err != nil {
				ui.ShowNotification(brand.DisplayName, "Stop failed: "+short(err.Error(), 100))
			}
		}()
	})
	ui.SetIcon(iconPNG).SetTooltip(brand.DisplayName + " daemon").SetMenu(menu)
	if runtime.GOOS == "darwin" {
		ui.SetTemplateIcon(templatePNG)
	}
	ui.OnDoubleClick(func() { go func() { _ = OpenUI() }() })
	ui.Show()

	ctx, cancel := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer cancel()
	go func() {
		<-ctx.Done()
		ui.Remove()
	}()
	go func() {
		tick := time.NewTicker(3 * time.Second)
		defer tick.Stop()
		for {
			s := capture(time.Now())
			stateItem.SetLabel(s.state)
			activityItem.SetLabel(s.activity)
			startItem.SetDisabled(!s.canStart)
			stopItem.SetDisabled(!s.canStop)
			ui.SetTooltip(brand.DisplayName + ": " + s.state)
			select {
			case <-ctx.Done():
				return
			case <-tick.C:
			}
		}
	}()
	return ui.Run()
}

func stopAndQuit(stop func() error, quit func()) error {
	if err := stop(); err != nil {
		return err
	}
	quit()
	return nil
}

func UIURL() string {
	return daemonctl.PublishedUIURL()
}

// OpenUI opens the URL served by the managed daemon in the default browser.
func OpenUI() error {
	return openDaemonUI(openURL)
}

func openDaemonUI(openBrowser func(string) error) error {
	url := UIURL()
	if url == "" {
		return fmt.Errorf("daemon UI is not ready; start the daemon and try again")
	}
	return openBrowser(url)
}

func openURL(url string) error {
	var cmd *exec.Cmd
	switch runtime.GOOS {
	case "darwin":
		cmd = exec.Command("open", url)
	case "windows":
		cmd = exec.Command("rundll32", "url.dll,FileProtocolHandler", url)
	default:
		cmd = exec.Command("xdg-open", url)
	}
	if err := cmd.Start(); err != nil {
		return err
	}
	go func() { _ = cmd.Wait() }()
	return nil
}
