package auth

import (
	"context"
	"net/http"
	"net/http/httptest"
	"reflect"
	"testing"
)

func TestProtectedResourceMetadataPathAndAudience(t *testing.T) {
	resource := ProtectedResource{Resource: "https://graphit.example.com:8443/team/mcp", AuthorizationServers: []string{"https://broker.example.com"}}
	if resource.MetadataPath() != "/.well-known/oauth-protected-resource/team/mcp" ||
		resource.MetadataURL() != "https://graphit.example.com:8443/.well-known/oauth-protected-resource/team/mcp" {
		t.Fatalf("metadata location=%q %q", resource.MetadataPath(), resource.MetadataURL())
	}
	if got := resource.AcceptedAudiences(); !reflect.DeepEqual(got, []string{resource.Resource}) {
		t.Fatalf("accepted audiences=%v", got)
	}
}

func TestBrokerResourceResolverRequiresConfiguredAdvertisedExactResource(t *testing.T) {
	target := "https://graphit.example.com:8443/mcp"
	other := "https://graphit.example.com:9443/mcp"
	var server *httptest.Server
	server = httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		document := brokerDiscoveryForTest(server.URL)
		document["authentication"].(map[string]any)["mcp_resources"] = []string{target, other}
		writeJSON(t, w, document)
	}))
	defer server.Close()
	provider := brokerProviderForTest(server.URL)
	provider.Broker.MCPResource = target
	resolver := &ProtectedResourceResolver{HTTP: server.Client()}
	result, ok := resolver.Resolve(context.Background(), provider, "graphit.example.com:8443")
	if !ok || result.Resource != target || !reflect.DeepEqual(result.AuthorizationServers, []string{server.URL}) || !reflect.DeepEqual(result.ScopesSupported, []string{"openid"}) {
		t.Fatalf("resource=%#v ok=%v", result, ok)
	}
	if audiences, ok := resolver.AcceptedAudiences(context.Background(), provider, "graphit.example.com:8443"); !ok || !reflect.DeepEqual(audiences, []string{target}) {
		t.Fatalf("audiences=%v ok=%v", audiences, ok)
	}
	for _, host := range []string{"graphit.example.com", "graphit.example.com:9443", "attacker.example.com:8443"} {
		if _, ok := resolver.Resolve(context.Background(), provider, host); ok {
			t.Fatalf("wrong host or port %q selected a resource", host)
		}
	}
	provider.Broker.MCPResource = "https://graphit.example.com:8443/other-path"
	if _, ok := resolver.Resolve(context.Background(), provider, "graphit.example.com:8443"); ok {
		t.Fatal("unadvertised resource path was selected")
	}
	provider.Broker.MCPResource = ""
	if _, ok := resolver.Resolve(context.Background(), provider, "graphit.example.com:8443"); ok {
		t.Fatal("resource was selected without explicit configuration")
	}
}
