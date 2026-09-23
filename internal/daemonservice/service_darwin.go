//go:build darwin

package daemonservice

import (
	"bytes"
	"encoding/xml"
	"os"
	"path/filepath"
	"strconv"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

var launchctlRun commandRunner = run

func launchLabel() string  { return serviceName() }
func launchTarget() string { return "gui/" + strconv.Itoa(os.Getuid()) + "/" + launchLabel() }
func launchDomain() string { return "gui/" + strconv.Itoa(os.Getuid()) }
func sourcePlist() string  { return filepath.Join(serviceDir(), launchLabel()+".plist") }
func loginPlist() (string, error) {
	home, err := os.UserHomeDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(home, "Library", "LaunchAgents", launchLabel()+".plist"), nil
}

func legacyLaunchLabel() string { return brand.Brand + ".daemon" }

func removeLegacyLaunchAgent() error {
	home, err := os.UserHomeDir()
	if err != nil {
		return err
	}
	path := filepath.Join(home, "Library", "LaunchAgents", legacyLaunchLabel()+".plist")
	target := launchDomain() + "/" + legacyLaunchLabel()
	if launchctlRun("launchctl", "print", target) == nil {
		if err := launchctlRun("launchctl", "bootout", target); err != nil {
			return err
		}
	}
	if err := os.Remove(path); err != nil && !os.IsNotExist(err) {
		return err
	}
	return nil
}

func xmlText(s string) string {
	var b bytes.Buffer
	_ = xml.EscapeText(&b, []byte(s))
	return b.String()
}

func launchPlist(exe string) []byte {
	return []byte(`<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>` + xmlText(launchLabel()) + `</string>
<key>ProgramArguments</key><array><string>` + xmlText(exe) + `</string><string>daemon</string><string>--managed</string></array>
<key>RunAtLoad</key><true/>
<key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict>
<key>StandardOutPath</key><string>/dev/null</string>
<key>StandardErrorPath</key><string>/dev/null</string>
</dict></plist>
`)
}

func install(autoStart bool) error {
	exe, err := ResolveExecutable()
	if err != nil {
		return err
	}
	path := sourcePlist()
	content := launchPlist(exe)
	if err := writeFileAtomic(path, content); err != nil {
		return err
	}
	login, err := loginPlist()
	if err != nil {
		return err
	}
	// Boot out an older registration of this service before replacing its plist.
	if launchctlRun("launchctl", "print", launchTarget()) == nil {
		if err := launchctlRun("launchctl", "bootout", launchTarget()); err != nil {
			return err
		}
	}
	if autoStart {
		if err := writeFileAtomic(login, content); err != nil {
			return err
		}
	} else if err := os.Remove(login); err != nil && !os.IsNotExist(err) {
		return err
	}
	// Retire the old periodic agent only after the replacement files are ready.
	return removeLegacyLaunchAgent()
}

func Start() error {
	if _, err := os.Stat(sourcePlist()); os.IsNotExist(err) {
		return ErrNotInstalled
	} else if err != nil {
		return err
	}
	if launchctlRun("launchctl", "print", launchTarget()) == nil {
		return nil
	}
	return launchctlRun("launchctl", "bootstrap", launchDomain(), sourcePlist())
}

func Stop() error {
	if _, err := os.Stat(sourcePlist()); os.IsNotExist(err) {
		return ErrNotInstalled
	} else if err != nil {
		return err
	}
	if launchctlRun("launchctl", "print", launchTarget()) != nil {
		return nil
	}
	return launchctlRun("launchctl", "bootout", launchTarget())
}

func Restart() error {
	if err := Stop(); err != nil {
		return err
	}
	return Start()
}

func Remove() error {
	if _, err := os.Stat(sourcePlist()); os.IsNotExist(err) {
		return nil
	} else if err != nil {
		return err
	}
	if err := Stop(); err != nil {
		return err
	}
	login, err := loginPlist()
	if err != nil {
		return err
	}
	if err := os.Remove(login); err != nil && !os.IsNotExist(err) {
		return err
	}
	if err := os.Remove(sourcePlist()); err != nil {
		return err
	}
	return nil
}

func GetStatus() (Status, error) {
	s := Status{Manager: "launchd LaunchAgent"}
	if _, err := os.Stat(sourcePlist()); os.IsNotExist(err) {
		return s, nil
	} else if err != nil {
		return s, err
	}
	s.Installed = true
	login, err := loginPlist()
	if err != nil {
		return s, err
	}
	if _, err := os.Stat(login); err == nil {
		s.AutoStart = true
	} else if !os.IsNotExist(err) {
		return s, err
	}
	s.Active = launchctlRun("launchctl", "print", launchTarget()) == nil
	return s, nil
}
