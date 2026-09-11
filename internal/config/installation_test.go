package config

import (
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func TestEnsureInstallationIdentityPersistsBothValues(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	t.Setenv(ConfigEnvVar(UnitIDKey), "")
	t.Setenv(ConfigEnvVar(ClientSecretConfigKey), "")
	resetUnitCache()

	if err := EnsureInstallationIdentity(); err != nil {
		t.Fatal(err)
	}
	for _, key := range []string{UnitIDKey, ClientSecretConfigKey} {
		value, ok, err := GetGlobalConfigValue(key)
		if err != nil || !ok || value == "" {
			t.Fatalf("%s = %q, ok=%t, err=%v", key, value, ok, err)
		}
	}
}
