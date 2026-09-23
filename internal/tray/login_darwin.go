//go:build darwin

package tray

import (
	"bytes"
	"encoding/xml"
	"os"
	"path/filepath"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/daemonservice"
)

func loginPath() (string, error) {
	home, err := os.UserHomeDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(home, "Library", "LaunchAgents", brand.Brand+"-tray.plist"), nil
}

func plistText(s string) string {
	var b bytes.Buffer
	_ = xml.EscapeText(&b, []byte(s))
	return b.String()
}

func InstallLogin() error {
	exe, err := daemonservice.ResolveExecutable()
	if err != nil {
		return err
	}
	path, err := loginPath()
	if err != nil {
		return err
	}
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return err
	}
	content := `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>` + plistText(brand.Brand+"-tray") + `</string>
<key>ProgramArguments</key><array><string>` + plistText(exe) + `</string><string>tray</string></array>
<key>RunAtLoad</key><true/>
</dict></plist>
`
	return os.WriteFile(path, []byte(content), 0o600)
}

func RemoveLogin() error {
	path, err := loginPath()
	if err != nil {
		return err
	}
	if err := os.Remove(path); err != nil && !os.IsNotExist(err) {
		return err
	}
	return nil
}

func LoginEnabled() (bool, error) {
	path, err := loginPath()
	if err != nil {
		return false, err
	}
	_, err = os.Stat(path)
	if os.IsNotExist(err) {
		return false, nil
	}
	return err == nil, err
}
