//go:build linux

package tray

import (
	"os"
	"path/filepath"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/daemonservice"
)

func loginPath() (string, error) {
	config := os.Getenv("XDG_CONFIG_HOME")
	if config == "" {
		home, err := os.UserHomeDir()
		if err != nil {
			return "", err
		}
		config = filepath.Join(home, ".config")
	}
	return filepath.Join(config, "autostart", brand.Brand+"-tray.desktop"), nil
}

func desktopQuote(s string) string {
	s = strings.ReplaceAll(s, `\`, `\\`)
	s = strings.ReplaceAll(s, `"`, `\"`)
	s = strings.ReplaceAll(s, `$`, `\$`)
	s = strings.ReplaceAll(s, "`", "\\`")
	s = strings.ReplaceAll(s, `%`, `%%`)
	return `"` + s + `"`
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
	content := "[Desktop Entry]\nType=Application\nName=" + brand.DisplayName + " tray\nExec=" + desktopQuote(exe) + " tray\nTerminal=false\nX-GNOME-Autostart-enabled=true\n"
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
