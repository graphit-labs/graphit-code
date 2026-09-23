package commands

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"slices"
	"strings"
	"testing"
	"time"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/agentpolicy"
	"github.com/graphit-labs/graphit-code/internal/auth"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hubaccess"
)

// brokerDiscovery serves the /.well-known/graphit-broker document a Graphit daemon reads to
// learn what it may advertise. Tests vary only the MCP resource list.
func brokerDiscovery(t *testing.T, resources []string) *httptest.Server {
	t.Helper()
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/.well-known/graphit-broker" {
			http.NotFound(w, r)
			return
		}
		issuer := "http://" + r.Host
		authentication := map[string]any{
			"type":                  "openid_connect",
			"issuer":                issuer,
			"client_id":             "graphit-cli",
			"scopes":                []string{"openid", "profile"},
			"redirect_uri_path":     "/oauth/callback",
			"audiences":             []string{"graphit-broker"},
			"access_token_audience": "graphit-broker",
		}
		if resources != nil {
			authentication["mcp_resources"] = resources
		}
		_ = json.NewEncoder(w).Encode(map[string]any{
			"version":        "1",
			"issuer":         issuer,
			"authentication": authentication,
		})
	}))
	t.Cleanup(server.Close)
	return server
}

// activateBrokerProvider makes a broker provider the active one for the process under test.
func activateBrokerProvider(t *testing.T, endpoint string) {
	t.Helper()
	store, err := auth.Open()
	if err != nil {
		t.Fatal(err)
	}
	provider := auth.Provider{
		Name: "broker", Type: auth.ProviderBroker,
		Broker: &auth.BrokerConfig{Endpoint: endpoint},
		AI: auth.AIConfig{
			Embedding: auth.AIServiceConfig{Mode: auth.ServiceBroker},
			Rerank:    auth.AIServiceConfig{Mode: auth.ServiceBroker},
		},
	}
	if err := store.AddProvider(provider); err != nil {
		t.Fatal(err)
	}
	if err := store.Login(auth.Profile{
		Name: "alice", Provider: provider.Name, Issuer: endpoint, Subject: "subject", Username: "alice",
		OIDC: &auth.OIDCSession{AccessToken: "profile-token", IDToken: "id-token", ExpiresAt: time.Now().Add(time.Hour)},
	}); err != nil {
		t.Fatal(err)
	}
}

func recordingMCPHandler(reached *bool) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		*reached = true
		w.WriteHeader(http.StatusNoContent)
	})
}

func TestMCPChallengeAdvertisesBrokerResourceMetadata(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	discovery := brokerDiscovery(t, []string{"https://graphit.example.com/mcp"})
	activateBrokerProvider(t, discovery.URL)

	reached := false
	mux := newDaemonMCPMux(recordingMCPHandler(&reached), daemonMCPMuxOptions{
		RuntimeKey: "runtime-key",
		Resolver:   auth.NewProtectedResourceResolver(),
	})

	recorder := httptest.NewRecorder()
	mux.ServeHTTP(recorder, httptest.NewRequest(http.MethodPost, "https://graphit.example.com/mcp", nil))

	if recorder.Code != http.StatusUnauthorized {
		t.Fatalf("status = %d; want %d", recorder.Code, http.StatusUnauthorized)
	}
	challenge := recorder.Header().Get("WWW-Authenticate")
	want := `resource_metadata="https://graphit.example.com/.well-known/oauth-protected-resource/mcp"`
	if !strings.HasPrefix(challenge, "Bearer ") || !strings.Contains(challenge, want) {
		t.Fatalf("WWW-Authenticate = %q; want Bearer challenge containing %s", challenge, want)
	}
	if reached {
		t.Fatal("unauthenticated request reached the MCP handler")
	}
}

func TestMCPMetadataDocumentDerivesFromBrokerDiscovery(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	discovery := brokerDiscovery(t, []string{"https://graphit.example.com/mcp"})
	activateBrokerProvider(t, discovery.URL)

	reached := false
	mux := newDaemonMCPMux(recordingMCPHandler(&reached), daemonMCPMuxOptions{
		RuntimeKey: "runtime-key",
		Resolver:   auth.NewProtectedResourceResolver(),
	})

	for _, path := range []string{"/.well-known/oauth-protected-resource", "/.well-known/oauth-protected-resource/mcp"} {
		recorder := httptest.NewRecorder()
		mux.ServeHTTP(recorder, httptest.NewRequest(http.MethodGet, "https://graphit.example.com"+path, nil))
		if recorder.Code != http.StatusOK {
			t.Fatalf("%s status = %d; want %d", path, recorder.Code, http.StatusOK)
		}
		var document struct {
			Resource               string   `json:"resource"`
			AuthorizationServers   []string `json:"authorization_servers"`
			ScopesSupported        []string `json:"scopes_supported"`
			BearerMethodsSupported []string `json:"bearer_methods_supported"`
		}
		if err := json.NewDecoder(recorder.Body).Decode(&document); err != nil {
			t.Fatalf("%s body is not JSON: %v", path, err)
		}
		if document.Resource != "https://graphit.example.com/mcp" {
			t.Errorf("%s resource = %q", path, document.Resource)
		}
		if len(document.AuthorizationServers) != 1 || document.AuthorizationServers[0] != discovery.URL {
			t.Errorf("%s authorization_servers = %v; want [%s]", path, document.AuthorizationServers, discovery.URL)
		}
		if strings.Join(document.ScopesSupported, ",") != "openid,profile" {
			t.Errorf("%s scopes_supported = %v; want the scopes the broker advertised", path, document.ScopesSupported)
		}
		if len(document.BearerMethodsSupported) != 1 || document.BearerMethodsSupported[0] != "header" {
			t.Errorf("%s bearer_methods_supported = %v", path, document.BearerMethodsSupported)
		}
	}
}

func TestMCPAdvertisesNothingWithoutResolvableResource(t *testing.T) {
	for _, testCase := range []struct {
		name      string
		resources []string
		host      string
		local     bool
	}{
		{name: "broker omits the field entirely", resources: nil, host: "https://graphit.example.com"},
		{name: "broker advertises an empty list", resources: []string{}, host: "https://graphit.example.com"},
		{name: "request host is not an advertised resource", resources: []string{"https://other.example.com/mcp"}, host: "https://graphit.example.com"},
		{name: "active provider is local", local: true, host: "https://graphit.example.com"},
	} {
		t.Run(testCase.name, func(t *testing.T) {
			t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
			if testCase.local {
				store, err := auth.Open()
				if err != nil {
					t.Fatal(err)
				}
				if err := store.AddProvider(auth.Provider{Name: "local", Type: auth.ProviderLocal, Local: &auth.LocalConfig{}}); err != nil {
					t.Fatal(err)
				}
				if err := store.Login(auth.Profile{Name: "alice", Provider: "local", Username: "alice", MCPKey: "local-key"}); err != nil {
					t.Fatal(err)
				}
			} else {
				discovery := brokerDiscovery(t, testCase.resources)
				activateBrokerProvider(t, discovery.URL)
			}

			reached := false
			mux := newDaemonMCPMux(recordingMCPHandler(&reached), daemonMCPMuxOptions{
				RuntimeKey: "runtime-key",
				Resolver:   auth.NewProtectedResourceResolver(),
			})

			metadata := httptest.NewRecorder()
			mux.ServeHTTP(metadata, httptest.NewRequest(http.MethodGet, testCase.host+"/.well-known/oauth-protected-resource", nil))
			if metadata.Code != http.StatusNotFound {
				t.Errorf("metadata status = %d; want %d when nothing may be advertised", metadata.Code, http.StatusNotFound)
			}

			challenge := httptest.NewRecorder()
			mux.ServeHTTP(challenge, httptest.NewRequest(http.MethodPost, testCase.host+"/mcp", nil))
			if challenge.Code != http.StatusUnauthorized {
				t.Errorf("challenge status = %d; want %d", challenge.Code, http.StatusUnauthorized)
			}
			if header := challenge.Header().Get("WWW-Authenticate"); strings.Contains(header, "resource_metadata") {
				t.Errorf("WWW-Authenticate = %q; want no resource_metadata", header)
			}

			// Bearer authentication must keep working in this state.
			authorized := httptest.NewRequest(http.MethodPost, testCase.host+"/mcp", nil)
			authorized.Header.Set("Authorization", "Bearer runtime-key")
			recorder := httptest.NewRecorder()
			mux.ServeHTTP(recorder, authorized)
			if recorder.Code != http.StatusNoContent || !reached {
				t.Errorf("runtime key status = %d, reached = %v; want the request to be served", recorder.Code, reached)
			}
		})
	}
}

func TestMCPSelectsAdvertisedResourceByRequestHost(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	discovery := brokerDiscovery(t, []string{"https://one.example.com/mcp", "https://two.example.com/mcp"})
	activateBrokerProvider(t, discovery.URL)

	reached := false
	mux := newDaemonMCPMux(recordingMCPHandler(&reached), daemonMCPMuxOptions{
		RuntimeKey: "runtime-key",
		Resolver:   auth.NewProtectedResourceResolver(),
	})

	for host, want := range map[string]string{
		"https://one.example.com": "https://one.example.com/mcp",
		"https://two.example.com": "https://two.example.com/mcp",
	} {
		recorder := httptest.NewRecorder()
		mux.ServeHTTP(recorder, httptest.NewRequest(http.MethodGet, host+"/.well-known/oauth-protected-resource", nil))
		if recorder.Code != http.StatusOK {
			t.Fatalf("%s metadata status = %d", host, recorder.Code)
		}
		var document struct {
			Resource string `json:"resource"`
		}
		if err := json.NewDecoder(recorder.Body).Decode(&document); err != nil {
			t.Fatal(err)
		}
		if document.Resource != want {
			t.Errorf("%s resource = %q; want %q", host, document.Resource, want)
		}
	}

	// A host the broker never vouched for selects nothing, so no value is derived from it.
	unknown := httptest.NewRecorder()
	mux.ServeHTTP(unknown, httptest.NewRequest(http.MethodGet, "https://attacker.example.com/.well-known/oauth-protected-resource", nil))
	if unknown.Code != http.StatusNotFound {
		t.Errorf("unadvertised host status = %d; want %d", unknown.Code, http.StatusNotFound)
	}
}

func TestMCPMetadataAndHealthNeedNoBearer(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	discovery := brokerDiscovery(t, []string{"https://graphit.example.com/mcp"})
	activateBrokerProvider(t, discovery.URL)

	reached := false
	mux := newDaemonMCPMux(recordingMCPHandler(&reached), daemonMCPMuxOptions{
		RuntimeKey: "runtime-key",
		Resolver:   auth.NewProtectedResourceResolver(),
	})

	for _, path := range []string{"/health", "/.well-known/oauth-protected-resource/mcp"} {
		recorder := httptest.NewRecorder()
		mux.ServeHTTP(recorder, httptest.NewRequest(http.MethodGet, "https://graphit.example.com"+path, nil))
		if recorder.Code != http.StatusOK {
			t.Errorf("%s status = %d; want %d without any Authorization header", path, recorder.Code, http.StatusOK)
		}
		if challenge := recorder.Header().Get("WWW-Authenticate"); challenge != "" {
			t.Errorf("%s emitted an authentication challenge: %q", path, challenge)
		}
	}
}

func TestMCPBearerKeepsRuntimeKeyLocalKeyAndIdentityContext(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	discovery := brokerDiscovery(t, []string{"https://graphit.example.com/mcp"})
	activateBrokerProvider(t, discovery.URL)

	var seen *http.Request
	mux := newDaemonMCPMux(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		seen = r
		w.WriteHeader(http.StatusNoContent)
	}), daemonMCPMuxOptions{
		RuntimeKey: "runtime-key",
		Resolver:   auth.NewProtectedResourceResolver(),
		Verifier: daemonVerifierFunc(func(_ context.Context, provider auth.Provider, token string, _ []string) (auth.VerifiedIdentity, error) {
			if provider.Type != auth.ProviderBroker || token != "broker-token" {
				return auth.VerifiedIdentity{}, errors.New("invalid token")
			}
			return auth.VerifiedIdentity{
				Issuer: discovery.URL, Subject: "caller", Username: "bob",
				Teams: []string{"platform"}, ExpiresAt: time.Now().Add(time.Hour),
			}, nil
		}),
	})

	// The daemon's own runtime key still authenticates, ahead of any token verification.
	runtime := httptest.NewRequest(http.MethodPost, "https://graphit.example.com/mcp", nil)
	runtime.Header.Set("Authorization", "Bearer runtime-key")
	recorder := httptest.NewRecorder()
	mux.ServeHTTP(recorder, runtime)
	if recorder.Code != http.StatusNoContent {
		t.Fatalf("runtime key status = %d; want %d", recorder.Code, http.StatusNoContent)
	}

	// A verified broker token carries its identity all the way to the MCP handler.
	seen = nil
	brokerRequest := httptest.NewRequest(http.MethodPost, "https://graphit.example.com/mcp", nil)
	brokerRequest.Header.Set("Authorization", "Bearer broker-token")
	recorder = httptest.NewRecorder()
	mux.ServeHTTP(recorder, brokerRequest)
	if recorder.Code != http.StatusNoContent || seen == nil {
		t.Fatalf("broker token status = %d, handler reached = %v", recorder.Code, seen != nil)
	}
	if got := auth.RequestBrokerBearer(seen.Context()); got != "broker-token" {
		t.Errorf("request broker bearer = %q; want the caller's own token", got)
	}
	identity, ok := auth.RequestIdentity(seen.Context())
	if !ok || identity.Username != "bob" {
		t.Errorf("request identity = %#v ok=%v; want bob", identity, ok)
	}
	subject, err := hubaccess.TrustedSubject(seen.Context())
	if err != nil || subject.UserID != "bob" || len(subject.TeamIDs) != 1 || subject.TeamIDs[0] != "platform" {
		t.Errorf("trusted subject = %#v err=%v", subject, err)
	}

	// An unrelated token is still refused.
	rejected := httptest.NewRequest(http.MethodPost, "https://graphit.example.com/mcp", nil)
	rejected.Header.Set("Authorization", "Bearer wrong")
	recorder = httptest.NewRecorder()
	mux.ServeHTTP(recorder, rejected)
	if recorder.Code != http.StatusUnauthorized {
		t.Errorf("invalid token status = %d; want %d", recorder.Code, http.StatusUnauthorized)
	}
}

func TestMCPBindsCapabilityProfileToScopedBearer(t *testing.T) {
	var seenProfile string
	mux := newDaemonMCPMux(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		seenProfile = r.Header.Get(agentpolicy.ProfileHeader)
		w.WriteHeader(http.StatusNoContent)
	}), daemonMCPMuxOptions{RuntimeKey: "runtime-key"})

	token, err := agentpolicy.MintCapabilityToken("runtime-key", agentpolicy.ProfileDreamMemory)
	if err != nil {
		t.Fatal(err)
	}
	request := httptest.NewRequest(http.MethodPost, "http://127.0.0.1/mcp", nil)
	request.Header.Set("Authorization", "Bearer "+token)
	request.Header.Set(agentpolicy.ProfileHeader, "unrestricted")
	recorder := httptest.NewRecorder()
	mux.ServeHTTP(recorder, request)
	if recorder.Code != http.StatusNoContent || seenProfile != agentpolicy.ProfileDreamMemory {
		t.Fatalf("scoped request status=%d profile=%q", recorder.Code, seenProfile)
	}

	seenProfile = "not-reached"
	request = httptest.NewRequest(http.MethodPost, "http://127.0.0.1/mcp", nil)
	request.Header.Set("Authorization", "Bearer runtime-key")
	request.Header.Set(agentpolicy.ProfileHeader, agentpolicy.ProfileDreamMemory)
	recorder = httptest.NewRecorder()
	mux.ServeHTTP(recorder, request)
	if recorder.Code != http.StatusNoContent || seenProfile != "" {
		t.Fatalf("master-key request status=%d retained client profile=%q", recorder.Code, seenProfile)
	}
}

// TestMCPWithoutInjectedVerifierRejectsUnknownToken covers the wiring production actually
// uses: no verifier is injected, so the handler must fall back to the real one. Sending a
// token that is neither the runtime key nor a local key drives that path, which previously
// dereferenced a nil interface and panicked the listener on the first real broker token.
func TestMCPWithoutInjectedVerifierRejectsUnknownToken(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	store, err := auth.Open()
	if err != nil {
		t.Fatal(err)
	}
	if err := store.AddProvider(auth.Provider{Name: "local", Type: auth.ProviderLocal, Local: &auth.LocalConfig{}}); err != nil {
		t.Fatal(err)
	}
	if err := store.Login(auth.Profile{Name: "alice", Provider: "local", Username: "alice", MCPKey: "local-key"}); err != nil {
		t.Fatal(err)
	}

	reached := false
	mux := newDaemonMCPMux(recordingMCPHandler(&reached), daemonMCPMuxOptions{
		RuntimeKey: "runtime-key",
		Resolver:   auth.NewProtectedResourceResolver(),
	})

	request := httptest.NewRequest(http.MethodPost, "https://graphit.example.com/mcp", nil)
	request.Header.Set("Authorization", "Bearer some-unrecognized-token")
	recorder := httptest.NewRecorder()
	mux.ServeHTTP(recorder, request)

	if recorder.Code != http.StatusUnauthorized {
		t.Fatalf("status = %d; want %d without panicking", recorder.Code, http.StatusUnauthorized)
	}
	if reached {
		t.Fatal("an unrecognized token reached the MCP handler")
	}
}

func TestMCPLocalProviderKeyAuthenticates(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	store, err := auth.Open()
	if err != nil {
		t.Fatal(err)
	}
	if err := store.AddProvider(auth.Provider{Name: "local", Type: auth.ProviderLocal, Local: &auth.LocalConfig{}}); err != nil {
		t.Fatal(err)
	}
	if err := store.Login(auth.Profile{Name: "alice", Provider: "local", Username: "alice", MCPKey: "local-key"}); err != nil {
		t.Fatal(err)
	}

	reached := false
	mux := newDaemonMCPMux(recordingMCPHandler(&reached), daemonMCPMuxOptions{
		RuntimeKey: "runtime-key",
		Resolver:   auth.NewProtectedResourceResolver(),
	})

	request := httptest.NewRequest(http.MethodPost, "https://graphit.example.com/mcp", nil)
	request.Header.Set("Authorization", "Bearer local-key")
	recorder := httptest.NewRecorder()
	mux.ServeHTTP(recorder, request)
	if recorder.Code != http.StatusNoContent || !reached {
		t.Fatalf("local MCP key status = %d, reached = %v", recorder.Code, reached)
	}
}

func TestMCPCORSPolicyFollowsDeclaredOrigins(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	discovery := brokerDiscovery(t, []string{"https://graphit.example.com/mcp"})
	activateBrokerProvider(t, discovery.URL)

	reached := false
	mux := newDaemonMCPMux(recordingMCPHandler(&reached), daemonMCPMuxOptions{
		RuntimeKey:     "runtime-key",
		Resolver:       auth.NewProtectedResourceResolver(),
		AllowedOrigins: []string{"https://claude.ai"},
	})

	t.Run("declared origin is allowed and the challenge is readable", func(t *testing.T) {
		request := httptest.NewRequest(http.MethodPost, "https://graphit.example.com/mcp", nil)
		request.Header.Set("Origin", "https://claude.ai")
		recorder := httptest.NewRecorder()
		mux.ServeHTTP(recorder, request)

		if got := recorder.Header().Get("Access-Control-Allow-Origin"); got != "https://claude.ai" {
			t.Errorf("Access-Control-Allow-Origin = %q; want the declared origin", got)
		}
		// Without this the browser hides the 401's challenge and discovery dies there.
		if got := recorder.Header().Get("Access-Control-Expose-Headers"); !strings.Contains(strings.ToLower(got), "www-authenticate") {
			t.Errorf("Access-Control-Expose-Headers = %q; want WWW-Authenticate exposed", got)
		}
	})

	t.Run("undeclared origin gets no allowance", func(t *testing.T) {
		request := httptest.NewRequest(http.MethodPost, "https://graphit.example.com/mcp", nil)
		request.Header.Set("Origin", "https://attacker.example")
		recorder := httptest.NewRecorder()
		mux.ServeHTTP(recorder, request)
		if got := recorder.Header().Get("Access-Control-Allow-Origin"); got != "" {
			t.Errorf("Access-Control-Allow-Origin = %q; want none", got)
		}
	})

	t.Run("preflight allows the headers the flow needs", func(t *testing.T) {
		request := httptest.NewRequest(http.MethodOptions, "https://graphit.example.com/mcp", nil)
		request.Header.Set("Origin", "https://claude.ai")
		request.Header.Set("Access-Control-Request-Method", http.MethodPost)
		// The Fetch standard has browsers send these lowercase and sorted, and rs/cors
		// relies on that ordering; a real preflight looks exactly like this.
		request.Header.Set("Access-Control-Request-Headers", "authorization,content-type,mcp-protocol-version,mcp-session-id")
		recorder := httptest.NewRecorder()
		mux.ServeHTTP(recorder, request)

		allowed := strings.ToLower(recorder.Header().Get("Access-Control-Allow-Headers"))
		for _, header := range []string{"authorization", "content-type", "mcp-session-id", "mcp-protocol-version"} {
			if !strings.Contains(allowed, header) {
				t.Errorf("Access-Control-Allow-Headers = %q; missing %s", allowed, header)
			}
		}
	})
}

func TestMCPWithoutDeclaredOriginsEmitsNoCORSHeaders(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	discovery := brokerDiscovery(t, []string{"https://graphit.example.com/mcp"})
	activateBrokerProvider(t, discovery.URL)

	reached := false
	mux := newDaemonMCPMux(recordingMCPHandler(&reached), daemonMCPMuxOptions{
		RuntimeKey: "runtime-key",
		Resolver:   auth.NewProtectedResourceResolver(),
	})

	request := httptest.NewRequest(http.MethodPost, "https://graphit.example.com/mcp", nil)
	request.Header.Set("Origin", "https://claude.ai")
	recorder := httptest.NewRecorder()
	mux.ServeHTTP(recorder, request)
	if got := recorder.Header().Get("Access-Control-Allow-Origin"); got != "" {
		t.Errorf("MCP Access-Control-Allow-Origin = %q; want none when no origin is declared", got)
	}

	// The metadata document stays public regardless: RFC 9728 section 3.1 exists so that an
	// unauthenticated client of any origin can discover where to authenticate.
	metadata := httptest.NewRequest(http.MethodGet, "https://graphit.example.com/.well-known/oauth-protected-resource/mcp", nil)
	metadata.Header.Set("Origin", "https://claude.ai")
	metadataRecorder := httptest.NewRecorder()
	mux.ServeHTTP(metadataRecorder, metadata)
	if got := metadataRecorder.Header().Get("Access-Control-Allow-Origin"); got != "*" {
		t.Errorf("metadata Access-Control-Allow-Origin = %q; want * for public discovery", got)
	}
}

// TestMCPTransportHeaderNamesMatchSDK pins the transport header names the CORS policy
// allows and exposes. The SDK keeps them unexported, so this drives the real handler and
// asserts it reacts to the exact strings we copied: if the SDK renames either one, the CORS
// policy would silently stop covering it and browser clients would break, so this fails.
func TestMCPTransportHeaderNamesMatchSDK(t *testing.T) {
	handler := mcp.NewStreamableHTTPHandler(func(*http.Request) *mcp.Server {
		return mcp.NewServer(&mcp.Implementation{Name: "graphit-test", Version: "1"}, nil)
	}, nil)

	initialize := `{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}`
	request := httptest.NewRequest(http.MethodPost, "https://graphit.example.com/mcp", strings.NewReader(initialize))
	request.Header.Set("Content-Type", "application/json")
	request.Header.Set("Accept", "application/json, text/event-stream")
	recorder := httptest.NewRecorder()
	handler.ServeHTTP(recorder, request)

	if got := recorder.Header().Get(mcpSessionIDHeader); got == "" {
		t.Errorf("the SDK answered initialize without a %s header; the constant no longer matches the transport", mcpSessionIDHeader)
	}

	// An unsupported value must be rejected, which only happens if the SDK reads the
	// protocol version from the header name we allow through CORS.
	versioned := httptest.NewRequest(http.MethodPost, "https://graphit.example.com/mcp", strings.NewReader(initialize))
	versioned.Header.Set("Content-Type", "application/json")
	versioned.Header.Set("Accept", "application/json, text/event-stream")
	versioned.Header.Set(mcpProtocolVersionHeader, "1999-01-01")
	versionRecorder := httptest.NewRecorder()
	handler.ServeHTTP(versionRecorder, versioned)
	if versionRecorder.Code != http.StatusBadRequest {
		t.Errorf("unsupported %s produced status %d; want %d, so the constant no longer matches the transport",
			mcpProtocolVersionHeader, versionRecorder.Code, http.StatusBadRequest)
	}
}

func TestMCPMetadataSurvivesBrokerOutage(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())

	reachable := true
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if !reachable {
			http.Error(w, "broker down", http.StatusServiceUnavailable)
			return
		}
		issuer := "http://" + r.Host
		_ = json.NewEncoder(w).Encode(map[string]any{
			"version": "1",
			"issuer":  issuer,
			"authentication": map[string]any{
				"type": "openid_connect", "issuer": issuer, "client_id": "graphit-cli",
				"scopes": []string{"openid", "profile"}, "redirect_uri_path": "/oauth/callback",
				"audiences": []string{"graphit-broker"}, "access_token_audience": "graphit-broker",
				"mcp_resources": []string{"https://graphit.example.com/mcp"},
			},
		})
	}))
	t.Cleanup(server.Close)
	activateBrokerProvider(t, server.URL)

	reached := false
	resolver := auth.NewProtectedResourceResolver()
	mux := newDaemonMCPMux(recordingMCPHandler(&reached), daemonMCPMuxOptions{RuntimeKey: "runtime-key", Resolver: resolver})

	warm := httptest.NewRecorder()
	mux.ServeHTTP(warm, httptest.NewRequest(http.MethodGet, "https://graphit.example.com/.well-known/oauth-protected-resource/mcp", nil))
	if warm.Code != http.StatusOK {
		t.Fatalf("first metadata status = %d", warm.Code)
	}

	reachable = false
	resolver.TTL = -time.Second // force the cache to be considered stale

	recorder := httptest.NewRecorder()
	mux.ServeHTTP(recorder, httptest.NewRequest(http.MethodGet, "https://graphit.example.com/.well-known/oauth-protected-resource/mcp", nil))
	if recorder.Code != http.StatusOK {
		t.Fatalf("metadata status during outage = %d; want the last good advertisement", recorder.Code)
	}
	var document struct {
		AuthorizationServers []string `json:"authorization_servers"`
	}
	if err := json.NewDecoder(recorder.Body).Decode(&document); err != nil {
		t.Fatal(err)
	}
	if len(document.AuthorizationServers) != 1 || document.AuthorizationServers[0] != server.URL {
		t.Fatalf("authorization_servers during outage = %v; want the broker that was verified earlier", document.AuthorizationServers)
	}
}

func TestMCPMetadataStaysSilentWhenBrokerWasNeverReachable(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		http.Error(w, "broker down", http.StatusServiceUnavailable)
	}))
	t.Cleanup(server.Close)
	activateBrokerProvider(t, server.URL)

	reached := false
	mux := newDaemonMCPMux(recordingMCPHandler(&reached), daemonMCPMuxOptions{
		RuntimeKey: "runtime-key",
		Resolver:   auth.NewProtectedResourceResolver(),
	})

	recorder := httptest.NewRecorder()
	mux.ServeHTTP(recorder, httptest.NewRequest(http.MethodGet, "https://graphit.example.com/.well-known/oauth-protected-resource/mcp", nil))
	if recorder.Code != http.StatusNotFound {
		t.Fatalf("metadata status = %d; want %d rather than a guessed authorization server", recorder.Code, http.StatusNotFound)
	}

	authorized := httptest.NewRequest(http.MethodPost, "https://graphit.example.com/mcp", nil)
	authorized.Header.Set("Authorization", "Bearer runtime-key")
	served := httptest.NewRecorder()
	mux.ServeHTTP(served, authorized)
	if served.Code != http.StatusNoContent || !reached {
		t.Fatalf("listener stopped serving during broker outage: status = %d", served.Code)
	}
}

// activateDirectOIDCProvider makes a direct OIDC provider the active one. Its issuer, scopes
// and MCP resource come from the provider's own configuration, which is what that provider
// type means: there is no broker to discover them from.
func activateDirectOIDCProvider(t *testing.T, issuer, resource string) {
	t.Helper()
	store, err := auth.Open()
	if err != nil {
		t.Fatal(err)
	}
	provider := auth.Provider{Name: "oidc", Type: auth.ProviderOIDC, OIDC: &auth.OIDCConfig{
		Issuer: issuer, ClientID: "graphit-cli", UsernameClaim: "preferred_username",
		Scopes: []string{"openid", "profile"}, MCPAudience: "graphit-mcp", MCPResource: resource,
	}}
	if err := store.AddProvider(provider); err != nil {
		t.Fatal(err)
	}
	if err := store.Login(auth.Profile{
		Name: "alice", Provider: provider.Name, Issuer: issuer, Subject: "subject", Username: "alice",
		OIDC: &auth.OIDCSession{AccessToken: "profile-token", IDToken: "id-token", ExpiresAt: time.Now().Add(time.Hour)},
	}); err != nil {
		t.Fatal(err)
	}
}

func TestDirectOIDCProviderServesChallengeAndMetadata(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	activateDirectOIDCProvider(t, "https://issuer.example/realms/acme", "https://graphit.example.com/mcp")

	reached := false
	mux := newDaemonMCPMux(recordingMCPHandler(&reached), daemonMCPMuxOptions{
		RuntimeKey: "runtime-key",
		Resolver:   auth.NewProtectedResourceResolver(),
	})

	challenge := httptest.NewRecorder()
	mux.ServeHTTP(challenge, httptest.NewRequest(http.MethodPost, "https://graphit.example.com/mcp", nil))
	if challenge.Code != http.StatusUnauthorized {
		t.Fatalf("status = %d; want %d", challenge.Code, http.StatusUnauthorized)
	}
	want := `resource_metadata="https://graphit.example.com/.well-known/oauth-protected-resource/mcp"`
	if got := challenge.Header().Get("WWW-Authenticate"); !strings.Contains(got, want) {
		t.Fatalf("WWW-Authenticate = %q; want %s", got, want)
	}

	metadata := httptest.NewRecorder()
	mux.ServeHTTP(metadata, httptest.NewRequest(http.MethodGet, "https://graphit.example.com/.well-known/oauth-protected-resource/mcp", nil))
	if metadata.Code != http.StatusOK {
		t.Fatalf("metadata status = %d", metadata.Code)
	}
	var document struct {
		Resource               string   `json:"resource"`
		AuthorizationServers   []string `json:"authorization_servers"`
		ScopesSupported        []string `json:"scopes_supported"`
		BearerMethodsSupported []string `json:"bearer_methods_supported"`
	}
	if err := json.NewDecoder(metadata.Body).Decode(&document); err != nil {
		t.Fatal(err)
	}
	if document.Resource != "https://graphit.example.com/mcp" {
		t.Errorf("resource = %q", document.Resource)
	}
	if len(document.AuthorizationServers) != 1 || document.AuthorizationServers[0] != "https://issuer.example/realms/acme" {
		t.Errorf("authorization_servers = %v; want the provider's own issuer", document.AuthorizationServers)
	}
	if strings.Join(document.ScopesSupported, ",") != "openid,profile" {
		t.Errorf("scopes_supported = %v", document.ScopesSupported)
	}
	if len(document.BearerMethodsSupported) != 1 || document.BearerMethodsSupported[0] != "header" {
		t.Errorf("bearer_methods_supported = %v", document.BearerMethodsSupported)
	}
}

func TestDirectOIDCProviderAcceptsResourceAudienceToken(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	activateDirectOIDCProvider(t, "https://issuer.example/realms/acme", "https://graphit.example.com/mcp")

	var seenAudiences []string
	var seen *http.Request
	mux := newDaemonMCPMux(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		seen = r
		w.WriteHeader(http.StatusNoContent)
	}), daemonMCPMuxOptions{
		RuntimeKey: "runtime-key",
		Resolver:   auth.NewProtectedResourceResolver(),
		Verifier: daemonVerifierFunc(func(_ context.Context, provider auth.Provider, token string, audiences []string) (auth.VerifiedIdentity, error) {
			seenAudiences = audiences
			if provider.Type != auth.ProviderOIDC || token != "agent-token" {
				return auth.VerifiedIdentity{}, errors.New("invalid token")
			}
			return auth.VerifiedIdentity{
				Issuer: "https://issuer.example/realms/acme", Subject: "caller", Username: "bob",
				Teams: []string{"platform"}, ExpiresAt: time.Now().Add(time.Hour),
			}, nil
		}),
	})

	request := httptest.NewRequest(http.MethodPost, "https://graphit.example.com/mcp", nil)
	request.Header.Set("Authorization", "Bearer agent-token")
	recorder := httptest.NewRecorder()
	mux.ServeHTTP(recorder, request)

	if recorder.Code != http.StatusNoContent || seen == nil {
		t.Fatalf("status = %d, handler reached = %v", recorder.Code, seen != nil)
	}
	// An agent that asked for the MCP resource as its RFC 8707 target gets a token whose aud
	// is that resource; the provider's own audience remains acceptable too.
	if !slices.Contains(seenAudiences, "https://graphit.example.com/mcp") {
		t.Errorf("accepted audiences = %v; want the canonical resource", seenAudiences)
	}
	if !slices.Contains(seenAudiences, "graphit-mcp") {
		t.Errorf("accepted audiences = %v; want the provider's configured audience", seenAudiences)
	}
	if identity, ok := auth.RequestIdentity(seen.Context()); !ok || identity.Username != "bob" {
		t.Errorf("request identity = %#v ok=%v", identity, ok)
	}
}
