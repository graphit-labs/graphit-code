package config

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func TestClientSecretIsGeneratedOnceAndPersisted(t *testing.T) {
	globalDir := t.TempDir()
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), globalDir)
	t.Setenv(ConfigEnvVar(ClientSecretConfigKey), "")

	const callers = 8
	values := make(chan string, callers)
	errs := make(chan error, callers)
	for range callers {
		go func() {
			value, err := ClientSecret()
			values <- value
			errs <- err
		}()
	}

	var first string
	for range callers {
		if err := <-errs; err != nil {
			t.Fatal(err)
		}
		value := <-values
		if value == "" {
			t.Fatal("ClientSecret returned empty")
		}
		if first == "" {
			first = value
		} else if value != first {
			t.Fatalf("concurrent callers received %q and %q", first, value)
		}
	}

	persisted, ok, err := GetGlobalConfigValue(ClientSecretConfigKey)
	if err != nil || !ok || persisted != first {
		t.Fatalf("persisted client.secret = %q, ok=%t, err=%v", persisted, ok, err)
	}
}

func TestClientSecretEnvironmentOverrideIsNotPersisted(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	t.Setenv(ConfigEnvVar(ClientSecretConfigKey), " deployment-salt ")

	if got, err := ClientSecret(); err != nil || got != "deployment-salt" {
		t.Fatalf("ClientSecret = %q, err=%v", got, err)
	}
	if _, ok, err := GetGlobalConfigValue(ClientSecretConfigKey); err != nil || ok {
		t.Fatalf("environment salt was persisted: ok=%t err=%v", ok, err)
	}
}

func TestClientSecretReportsPersistenceFailure(t *testing.T) {
	badDir := filepath.Join(t.TempDir(), "not-a-directory")
	if err := os.WriteFile(badDir, []byte("x"), 0o600); err != nil {
		t.Fatal(err)
	}
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), badDir)
	t.Setenv(ConfigEnvVar(ClientSecretConfigKey), "")

	if got, err := ClientSecret(); err == nil || got != "" {
		t.Fatalf("ClientSecret = %q, err=%v; want an empty value and persistence error", got, err)
	}
}

func TestResolveHubEventsAnonymizeDefaultsToFalse(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	t.Setenv(ConfigEnvVar(HubEventsAnonymizeConfigKey), "")

	if ResolveHubEventsAnonymize(nil, nil) {
		t.Fatal("event anonymization must be opt-in")
	}
}

func TestResolveHubEventsAnonymizeUsesConfigurationPrecedence(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	t.Setenv(ConfigEnvVar(HubEventsAnonymizeConfigKey), "")
	if err := SetGlobalConfigValue(HubEventsAnonymizeConfigKey, "true"); err != nil {
		t.Fatal(err)
	}
	if !ResolveHubEventsAnonymize(nil, nil) {
		t.Fatal("global true did not enable event anonymization")
	}

	t.Setenv(ConfigEnvVar(HubEventsAnonymizeConfigKey), "false")
	if ResolveHubEventsAnonymize(nil, nil) {
		t.Fatal("environment false did not override global true")
	}

	inline := ConfigMap{}
	SetConfigValue(inline, HubEventsAnonymizeConfigKey, "true")
	if !ResolveHubEventsAnonymize(inline, nil) {
		t.Fatal("inline true did not override environment false")
	}
}
