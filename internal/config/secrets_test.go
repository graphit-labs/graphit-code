package config

import (
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// Every secret must be suppliable through the environment. That is the only channel that does
// not persist into a container layer, a config file, or a shell history entry, so a credential
// that can only be reached through `config set` is not deployable.
//
// The loop is over SecretConfigKeys rather than over a hand-written list of three, so a fourth
// credential added to that list is covered here the moment it is added.
func TestEverySecretKeyResolvesFromItsEnvironmentVariable(t *testing.T) {
	for _, key := range SecretConfigKeys {
		t.Run(key, func(t *testing.T) {
			t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
			t.Setenv(ConfigEnvVar(key), "from-the-environment")

			if got := ResolveConfig(key, nil, nil); got != "from-the-environment" {
				t.Fatalf("ResolveConfig(%q) = %q, want the environment value", key, got)
			}
		})
	}
}

// The environment outranks the global config file. This is what lets one image serve several
// deployments: the file holds whatever setup wrote, and `-e` overrides it per container without
// rewriting anything on disk.
func TestSecretEnvironmentVariableOutranksTheGlobalConfigFile(t *testing.T) {
	for _, key := range SecretConfigKeys {
		t.Run(key, func(t *testing.T) {
			t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())

			if err := SetGlobalConfigValue(key, "from-the-file"); err != nil {
				t.Fatalf("SetGlobalConfigValue(%q): %v", key, err)
			}
			if got := ResolveConfig(key, nil, nil); got != "from-the-file" {
				t.Fatalf("with no environment variable, ResolveConfig(%q) = %q", key, got)
			}

			t.Setenv(ConfigEnvVar(key), "from-the-environment")
			if got := ResolveConfig(key, nil, nil); got != "from-the-environment" {
				t.Fatalf("ResolveConfig(%q) = %q, want the environment to win", key, got)
			}
		})
	}
}

// An empty environment variable is not an answer — ResolveConfig skips it. A container image can
// therefore declare every secret variable empty for discoverability without that declaration
// blanking a value the config file holds.
func TestAnEmptySecretEnvironmentVariableDoesNotMaskTheConfigFile(t *testing.T) {
	for _, key := range SecretConfigKeys {
		t.Run(key, func(t *testing.T) {
			t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
			if err := SetGlobalConfigValue(key, "from-the-file"); err != nil {
				t.Fatalf("SetGlobalConfigValue(%q): %v", key, err)
			}

			t.Setenv(ConfigEnvVar(key), "")
			if got := ResolveConfig(key, nil, nil); got != "from-the-file" {
				t.Fatalf("ResolveConfig(%q) = %q, want the file value to survive", key, got)
			}
		})
	}
}

func TestEverySecretKeyIsRedacted(t *testing.T) {
	for _, key := range SecretConfigKeys {
		if !IsSecretConfigKey(key) {
			t.Errorf("%q is in SecretConfigKeys but IsSecretConfigKey says it is not a secret", key)
		}
		if !IsSecretConfigKey(strings.ToUpper(key)) {
			t.Errorf("IsSecretConfigKey(%q) must not depend on case", strings.ToUpper(key))
		}
	}
}

// The access key ID is intentionally NOT a secret. Pinning that here means a future change that
// starts redacting it has to be a deliberate edit to this test rather than a side effect.
func TestTheAccessKeyIDIsNotTreatedAsASecret(t *testing.T) {
	if IsSecretConfigKey("hub.access_key_id") {
		t.Fatal("hub.access_key_id is an identifier, not a secret — see SecretConfigKeys")
	}
	if IsSecretConfigKey("ide") || IsSecretConfigKey("") {
		t.Fatal("a non-credential key was reported as a secret")
	}
}

// The env-var names come from ConfigEnvVar — the one derivation ResolveConfig itself uses. This
// pins the rule, and pins that secrets have no separate naming scheme of their own.
func TestConfigEnvVarFollowsTheResolverRule(t *testing.T) {
	cases := map[string]string{
		"hub.secret_access_key": brand.EnvPrefix() + "_HUB_SECRET_ACCESS_KEY",
		"ai.embedding.api_key":  brand.EnvPrefix() + "_AI_EMBEDDING_API_KEY",
		"ai.rerank.api_key":     brand.EnvPrefix() + "_AI_RERANK_API_KEY",
		"hub.bucket":            brand.EnvPrefix() + "_HUB_BUCKET",
		"modules.agent":         brand.EnvPrefix() + "_MODULES_AGENT",
		"ui.host":               brand.EnvPrefix() + "_UI_HOST",
	}
	for key, want := range cases {
		if got := ConfigEnvVar(key); got != want {
			t.Errorf("ConfigEnvVar(%q) = %q, want %q", key, got, want)
		}
	}

	if got := SecretConfigEnvVars(); len(got) != len(SecretConfigKeys) {
		t.Fatalf("SecretConfigEnvVars returned %d entries for %d keys", len(got), len(SecretConfigKeys))
	}
	for i, key := range SecretConfigKeys {
		if SecretConfigEnvVars()[i] != ConfigEnvVar(key) {
			t.Errorf("SecretConfigEnvVars()[%d] does not match ConfigEnvVar(%q)", i, key)
		}
	}
}
