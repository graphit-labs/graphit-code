package auth

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
)

func TestBrokerDiscoveryDecodesWithoutMCPResources(t *testing.T) {
	// A broker that predates the MCP resource list is a valid broker, not a broken one:
	// it simply has nothing for this daemon to advertise.
	var discovery BrokerDiscovery
	body := `{"version":"1","authentication":{"type":"openid_connect","issuer":"https://broker.example","client_id":"graphit-cli","access_token_audience":"graphit-broker"}}`
	if err := json.Unmarshal([]byte(body), &discovery); err != nil {
		t.Fatalf("discovery without mcp_resources failed to decode: %v", err)
	}
	if discovery.Authentication.MCPResources != nil {
		t.Fatalf("MCPResources = %v; want nil", discovery.Authentication.MCPResources)
	}
	if discovery.Authentication.AccessTokenAudience != "graphit-broker" {
		t.Fatalf("the rest of the document did not survive decoding: %#v", discovery.Authentication)
	}
}

func TestSelectAdvertisedResourceRequiresAnAdvertisedHost(t *testing.T) {
	resources := []string{"https://one.example.com/mcp", "https://two.example.com/mcp"}
	for _, testCase := range []struct {
		name      string
		resources []string
		host      string
		want      string
		wantOK    bool
	}{
		{name: "matching host selects its own resource", resources: resources, host: "one.example.com", want: "https://one.example.com/mcp", wantOK: true},
		{name: "matching host with a port still selects", resources: resources, host: "two.example.com:8443", want: "https://two.example.com/mcp", wantOK: true},
		{name: "host casing is irrelevant", resources: resources, host: "ONE.example.com", want: "https://one.example.com/mcp", wantOK: true},
		{name: "unadvertised host selects nothing", resources: resources, host: "attacker.example.com"},
		{name: "a lone resource is not assumed to be this daemon", resources: []string{"https://other.example.com/mcp"}, host: "graphit.example.com"},
		{name: "empty list selects nothing", resources: nil, host: "one.example.com"},
		{name: "missing host selects nothing", resources: resources},
	} {
		t.Run(testCase.name, func(t *testing.T) {
			got, ok := selectAdvertisedResource(testCase.resources, testCase.host, true)
			if ok != testCase.wantOK || got != testCase.want {
				t.Fatalf("selectAdvertisedResource(%v, %q, true) = (%q, %v); want (%q, %v)",
					testCase.resources, testCase.host, got, ok, testCase.want, testCase.wantOK)
			}
		})
	}
}

func TestProtectedResourceMetadataLocation(t *testing.T) {
	for _, testCase := range []struct {
		resource string
		wantPath string
		wantURL  string
	}{
		{
			resource: "https://graphit.example.com/mcp",
			wantPath: "/.well-known/oauth-protected-resource/mcp",
			wantURL:  "https://graphit.example.com/.well-known/oauth-protected-resource/mcp",
		},
		{
			resource: "https://graphit.example.com",
			wantPath: "/.well-known/oauth-protected-resource",
			wantURL:  "https://graphit.example.com/.well-known/oauth-protected-resource",
		},
	} {
		resource := ProtectedResource{Resource: testCase.resource}
		if got := resource.MetadataPath(); got != testCase.wantPath {
			t.Errorf("MetadataPath(%q) = %q; want %q", testCase.resource, got, testCase.wantPath)
		}
		if got := resource.MetadataURL(); got != testCase.wantURL {
			t.Errorf("MetadataURL(%q) = %q; want %q", testCase.resource, got, testCase.wantURL)
		}
	}
}

func TestProtectedResourceResolverRejectsForeignIssuer(t *testing.T) {
	// A tampered discovery document must not be able to point MCP clients at an
	// authorization server outside the broker's own origin.
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		_ = json.NewEncoder(w).Encode(map[string]any{
			"version": "1",
			"authentication": map[string]any{
				"type":                  "openid_connect",
				"issuer":                "https://attacker.example",
				"client_id":             "graphit-cli",
				"scopes":                []string{"openid", "profile"},
				"access_token_audience": "graphit-broker",
				"mcp_resources":         []string{"https://graphit.example.com/mcp"},
			},
		})
	}))
	defer server.Close()

	provider := Provider{Name: "broker", Type: ProviderBroker, Broker: &BrokerConfig{Endpoint: server.URL}}
	resolver := &ProtectedResourceResolver{TTL: time.Minute}
	if _, ok := resolver.Resolve(context.Background(), provider, "graphit.example.com"); ok {
		t.Fatal("resolver accepted an issuer outside the broker origin")
	}
}

func TestProtectedResourceResolverAdvertisesNothingWithoutAResource(t *testing.T) {
	resolver := NewProtectedResourceResolver()
	for _, testCase := range []struct {
		name     string
		provider Provider
	}{
		{name: "local provider has no authorization server", provider: Provider{Name: "local", Type: ProviderLocal, Local: &LocalConfig{}}},
		{
			name:     "direct OIDC without a configured resource",
			provider: Provider{Name: "oidc", Type: ProviderOIDC, OIDC: &OIDCConfig{Issuer: "https://issuer.example", MCPAudience: "graphit-mcp"}},
		},
		{
			name:     "direct OIDC without an issuer",
			provider: Provider{Name: "oidc", Type: ProviderOIDC, OIDC: &OIDCConfig{MCPResource: "https://graphit.example.com/mcp"}},
		},
	} {
		t.Run(testCase.name, func(t *testing.T) {
			if _, ok := resolver.Resolve(context.Background(), testCase.provider, "graphit.example.com"); ok {
				t.Error("an advertisement was produced with nothing to advertise")
			}
			if _, ok := resolver.AcceptedAudiences(context.Background(), testCase.provider, "graphit.example.com"); ok {
				t.Error("an audience set was produced with nothing to advertise")
			}
		})
	}
}

func TestDirectOIDCProviderAdvertisesItsOwnConfiguration(t *testing.T) {
	// A direct OIDC provider is configured on this daemon, so its own settings are the
	// authority, exactly as broker discovery is for a broker-managed provider.
	provider := Provider{Name: "oidc", Type: ProviderOIDC, OIDC: &OIDCConfig{
		Issuer:      "https://issuer.example/realms/acme/",
		Scopes:      []string{"openid", "profile", "email"},
		MCPResource: "https://graphit.example.com/mcp",
		MCPAudience: "graphit-mcp",
	}}
	resolver := NewProtectedResourceResolver()

	resolved, ok := resolver.Resolve(context.Background(), provider, "graphit.example.com")
	if !ok {
		t.Fatal("direct OIDC provider advertised nothing")
	}
	if resolved.Resource != "https://graphit.example.com/mcp" {
		t.Errorf("resource = %q", resolved.Resource)
	}
	// The trailing slash must not survive into the advertised issuer.
	if len(resolved.AuthorizationServers) != 1 || resolved.AuthorizationServers[0] != "https://issuer.example/realms/acme" {
		t.Errorf("authorization_servers = %v", resolved.AuthorizationServers)
	}
	if strings.Join(resolved.ScopesSupported, ",") != "openid,profile,email" {
		t.Errorf("scopes_supported = %v", resolved.ScopesSupported)
	}
	if got := resolved.MetadataURL(); got != "https://graphit.example.com/.well-known/oauth-protected-resource/mcp" {
		t.Errorf("metadata URL = %q", got)
	}

	// Both the RFC 8707 resource and the provider's own audience are legitimate.
	audiences, ok := resolver.AcceptedAudiences(context.Background(), provider, "graphit.example.com")
	if !ok || strings.Join(audiences, ",") != "https://graphit.example.com/mcp,graphit-mcp" {
		t.Fatalf("accepted audiences = %v ok=%v", audiences, ok)
	}

	// Its resource is specific to this daemon, so it does not depend on the request host the
	// way a broker's shared list does.
	if _, ok := resolver.Resolve(context.Background(), provider, "reached-by-another-name.example"); !ok {
		t.Error("a direct OIDC provider stopped advertising when reached by another name")
	}
}

func TestBrokerAcceptedAudiencesCoverResourceAndBrokerAudience(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		issuer := "http://" + r.Host
		_ = json.NewEncoder(w).Encode(map[string]any{
			"version": "1", "issuer": issuer,
			"authentication": map[string]any{
				"type": "openid_connect", "issuer": issuer, "client_id": "graphit-cli",
				"scopes": []string{"openid", "profile"}, "access_token_audience": "graphit-broker",
				"mcp_resources": []string{"https://graphit.example.com/mcp"},
			},
		})
	}))
	defer server.Close()

	provider := Provider{Name: "broker", Type: ProviderBroker, Broker: &BrokerConfig{Endpoint: server.URL}}
	audiences, ok := NewProtectedResourceResolver().AcceptedAudiences(context.Background(), provider, "graphit.example.com")
	if !ok || strings.Join(audiences, ",") != "https://graphit.example.com/mcp,graphit-broker" {
		t.Fatalf("accepted audiences = %v ok=%v; want the resource and the broker audience", audiences, ok)
	}
}

func TestProtectedResourceResolverCachesAndSurvivesOutage(t *testing.T) {
	reachable := true
	calls := 0
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		calls++
		if !reachable {
			http.Error(w, "down", http.StatusServiceUnavailable)
			return
		}
		issuer := "http://" + r.Host
		_ = json.NewEncoder(w).Encode(map[string]any{
			"version": "1", "issuer": issuer,
			"authentication": map[string]any{
				"type": "openid_connect", "issuer": issuer, "client_id": "graphit-cli",
				"scopes": []string{"openid", "profile"}, "access_token_audience": "graphit-broker",
				"mcp_resources": []string{"https://graphit.example.com/mcp"},
			},
		})
	}))
	defer server.Close()

	provider := Provider{Name: "broker", Type: ProviderBroker, Broker: &BrokerConfig{Endpoint: server.URL}}
	resolver := &ProtectedResourceResolver{TTL: time.Hour}

	if _, ok := resolver.Resolve(context.Background(), provider, "graphit.example.com"); !ok {
		t.Fatal("first resolve failed")
	}
	if _, ok := resolver.Resolve(context.Background(), provider, "graphit.example.com"); !ok {
		t.Fatal("second resolve failed")
	}
	if calls != 1 {
		t.Fatalf("broker was contacted %d times; want the advertisement to be cached", calls)
	}

	reachable = false
	resolver.TTL = -time.Second
	resolved, ok := resolver.Resolve(context.Background(), provider, "graphit.example.com")
	if !ok || resolved.Resource != "https://graphit.example.com/mcp" {
		t.Fatalf("resolve during outage = (%#v, %v); want the last good advertisement", resolved, ok)
	}

	// A different provider must not inherit the cached deployment's identity.
	other := Provider{Name: "other", Type: ProviderBroker, Broker: &BrokerConfig{Endpoint: server.URL}}
	if _, ok := resolver.Resolve(context.Background(), other, "graphit.example.com"); ok {
		t.Fatal("cached advertisement leaked to a different provider")
	}
}
