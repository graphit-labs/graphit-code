package config

import (
	"fmt"
	"strings"
	"sync"

	"github.com/oklog/ulid/v2"
)

// HubEventsAnonymizeConfigKey controls whether Hub event payloads replace project and user IDs
// with stable, installation-specific hashes. It is opt-in: an absent or non-true value keeps the
// identifiers explicit.
const HubEventsAnonymizeConfigKey = "hub.events.anonymize"

// ClientSecretConfigKey is the installation salt used to pseudonymize event identifiers.
const ClientSecretConfigKey = "client.secret"

var clientSecretMu sync.Mutex

// ClientSecret returns the installation salt, generating and persisting one when absent.
// Environment overrides keep their normal precedence and are never copied into the config file.
func ClientSecret() (string, error) {
	if secret := strings.TrimSpace(ResolveConfig(ClientSecretConfigKey, nil, nil)); secret != "" {
		return secret, nil
	}

	clientSecretMu.Lock()
	defer clientSecretMu.Unlock()
	if secret := strings.TrimSpace(ResolveConfig(ClientSecretConfigKey, nil, nil)); secret != "" {
		return secret, nil
	}

	generated := ulid.Make().String()
	if err := SetGlobalConfigValue(ClientSecretConfigKey, generated); err != nil {
		return "", fmt.Errorf("persisting the event client secret: %w", err)
	}
	return generated, nil
}

// ResolveHubEventsAnonymize resolves the event anonymization switch through the ordinary
// configuration precedence chain. Only an explicit true enables anonymization.
func ResolveHubEventsAnonymize(inlineCfg, projectCfg ConfigMap) bool {
	return strings.EqualFold(ResolveConfig(HubEventsAnonymizeConfigKey, inlineCfg, projectCfg), "true")
}

// HubEventsAnonymize resolves the machine's effective event anonymization setting.
func HubEventsAnonymize() bool { return ResolveHubEventsAnonymize(nil, nil) }
