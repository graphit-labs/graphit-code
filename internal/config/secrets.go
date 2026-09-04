package config

import "strings"

// SecretConfigKeys is the canonical list of configuration keys whose value is a credential.
//
// It exists so the two things a credential needs are derived from ONE list instead of being
// remembered separately at each site: every key here is redacted wherever configuration is
// printed, and every key here resolves from its environment variable. Adding a credential key
// anywhere in the tree means adding it here, and secrets_test.go fails if the two drift.
//
// `hub.access_key_id` is deliberately absent. An access key ID is an identifier, not a secret —
// AWS treats it as one half of a pair in which only the other half is confidential — and
// redacting it would hide the value an operator most often needs to read back when diagnosing
// which credentials a machine is actually using.
var SecretConfigKeys = []string{
	"hub.secret_access_key",
	"ai.embedding.api_key",
	"ai.rerank.api_key",
	MCPAPIKeyConfigKey,
}

// IsSecretConfigKey reports whether a key's value must be redacted when configuration is
// printed. `graphit config get` and `graphit config list` both consult it.
func IsSecretConfigKey(key string) bool {
	normalized := strings.ToLower(strings.TrimSpace(key))
	for _, secret := range SecretConfigKeys {
		if normalized == secret {
			return true
		}
	}
	return false
}

// SecretConfigEnvVars returns the environment variable that supplies each secret key, in the
// order of SecretConfigKeys. Container images and deployment documentation use it to enumerate
// the credentials an operator may need to pass in.
//
// The names come from ConfigEnvVar — the same derivation ResolveConfig itself uses — so there is
// no second rule here to drift from the first. Secrets need no separate env-var mechanism: they
// already resolve through the ordinary chain, in which the environment outranks both the project
// lockfile and the global config file.
func SecretConfigEnvVars() []string {
	vars := make([]string, 0, len(SecretConfigKeys))
	for _, key := range SecretConfigKeys {
		vars = append(vars, ConfigEnvVar(key))
	}
	return vars
}
