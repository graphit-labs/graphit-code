package auth

import (
	"context"
	"testing"
)

func TestBrokerCredentialResolverForwardsCallerBearer(t *testing.T) {
	resolver := &BrokerCredentialResolver{}
	snapshot := Snapshot{Provider: Provider{Type: ProviderBroker, Broker: &BrokerConfig{Endpoint: "https://broker.example"}},
		Profile: Profile{OIDC: &OIDCSession{AccessToken: "daemon-login-token"}}}
	for _, caller := range []string{"mcp-client-a", "mcp-client-b"} {
		got, err := resolver.Resolve(WithBrokerBearer(context.Background(), caller), snapshot)
		if err != nil || got != caller {
			t.Fatalf("caller=%q forwarded=%q err=%v", caller, got, err)
		}
	}
	got, err := resolver.Resolve(context.Background(), snapshot)
	if err != nil || got != "daemon-login-token" {
		t.Fatalf("daemon login token=%q err=%v", got, err)
	}
}
