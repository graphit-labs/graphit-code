package commands

import (
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/config"
)

func TestConfigOutputRedactsTheS3Secret(t *testing.T) {
	_, cleanup := setupTestHome(t)
	defer cleanup()

	if _, err := executeCommand("config", "--global", "hub.secret_access_key", "never-print-this"); err != nil {
		t.Fatalf("set secret: %v", err)
	}
	out, err := executeCommand("config", "--list", "--global")
	if err != nil {
		t.Fatalf("list global config: %v", err)
	}
	if strings.Contains(out, "never-print-this") || !strings.Contains(out, "***") {
		t.Fatalf("secret was not redacted: %q", out)
	}
}

// TestConfigOutputRedactsEverySecretKey walks config.SecretConfigKeys rather than naming the S3
// secret alone, which is the gap this replaces: the AI provider API keys were stored by setup and
// printed in clear by `config get` and `config --list`, because redaction knew about exactly one
// key. Driving the assertion from the canonical list means a credential added there is covered
// here without anyone remembering to come back.
func TestConfigOutputRedactsEverySecretKey(t *testing.T) {
	for _, key := range config.SecretConfigKeys {
		t.Run(key, func(t *testing.T) {
			_, cleanup := setupTestHome(t)
			defer cleanup()

			const value = "never-print-this"
			setOutput, err := executeCommand("config", "--global", key, value)
			if err != nil {
				t.Fatalf("set %s: %v", key, err)
			}
			if strings.Contains(setOutput, value) {
				t.Fatalf("%s leaked into set output: %q", key, setOutput)
			}

			listed, err := executeCommand("config", "--list", "--global")
			if err != nil {
				t.Fatalf("list global config: %v", err)
			}
			if strings.Contains(listed, value) {
				t.Fatalf("%s leaked into --list output: %q", key, listed)
			}
			if !strings.Contains(listed, "***") {
				t.Fatalf("%s was not redacted in --list output: %q", key, listed)
			}

			got, err := executeCommand("config", "--global", "--get", key)
			if err != nil {
				t.Fatalf("get %s: %v", key, err)
			}
			if strings.Contains(got, value) {
				t.Fatalf("%s leaked into --get output: %q", key, got)
			}
		})
	}
}
