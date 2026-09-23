//go:build darwin

package daemonservice

import (
	"encoding/xml"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestLaunchPlistUsesManagedRestartWithoutPolling(t *testing.T) {
	content := launchPlist(`/Applications/Graphit & Co/graphit`)
	var root struct{ XMLName xml.Name }
	if err := xml.Unmarshal(content, &root); err != nil {
		t.Fatalf("invalid plist XML: %v", err)
	}
	text := string(content)
	for _, want := range []string{`<key>KeepAlive</key>`, `<key>SuccessfulExit</key><false/>`, `<key>RunAtLoad</key><true/>`, `Graphit &amp; Co`} {
		if !strings.Contains(text, want) {
			t.Errorf("missing %q", want)
		}
	}
	if strings.Contains(text, "StartInterval") {
		t.Fatal("periodic watchdog remains")
	}
}

func TestLegacyLaunchAgentMigration(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("HOME", dir)
	path := filepath.Join(dir, "Library", "LaunchAgents", legacyLaunchLabel()+".plist")
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte("old periodic agent"), 0o600); err != nil {
		t.Fatal(err)
	}
	old := launchctlRun
	var bootedOut bool
	launchctlRun = func(_ string, args ...string) error {
		if len(args) > 0 && args[0] == "print" {
			return nil
		}
		if len(args) > 0 && args[0] == "bootout" {
			bootedOut = true
			return nil
		}
		return errors.New("unexpected launchctl command")
	}
	t.Cleanup(func() { launchctlRun = old })
	if err := removeLegacyLaunchAgent(); err != nil {
		t.Fatal(err)
	}
	if !bootedOut {
		t.Fatal("legacy agent was not booted out")
	}
	if _, err := os.Stat(path); !os.IsNotExist(err) {
		t.Fatalf("legacy plist remains: %v", err)
	}
}
