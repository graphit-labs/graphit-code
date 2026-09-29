package auth

import (
	"context"
	"crypto/ed25519"
	"crypto/rand"
	"encoding/base64"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"net/http/httptest"
	"net/url"
	"strings"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func TestOIDCVerifiesBrokerEdDSAIDToken(t *testing.T) {
	publicKey, privateKey, err := ed25519.GenerateKey(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	now := time.Now().UTC()
	var server *httptest.Server
	server = httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/.well-known/openid-configuration":
			writeJSON(t, w, map[string]any{"issuer": server.URL, "authorization_endpoint": server.URL + "/authorize", "token_endpoint": server.URL + "/token", "jwks_uri": server.URL + "/jwks"})
		case "/jwks":
			writeJSON(t, w, map[string]any{"keys": []any{map[string]any{"kty": "OKP", "kid": "broker", "alg": "EdDSA", "use": "sig", "crv": "Ed25519", "x": base64.RawURLEncoding.EncodeToString(publicKey)}}})
		default:
			http.NotFound(w, r)
		}
	}))
	defer server.Close()
	client := &OIDCClient{HTTP: server.Client(), Now: func() time.Time { return now }}
	discovery, err := client.Discovery(context.Background(), server.URL)
	if err != nil {
		t.Fatal(err)
	}
	resource := server.URL + "/v1"
	claims := map[string]any{"iss": server.URL, "sub": "gb_sub_1", "aud": []string{"graphit-broker", resource, "graphit-cli"}, "azp": "graphit-cli", "iat": now.Unix(), "exp": now.Add(time.Hour).Unix(), "nonce": "nonce", "preferred_username": "alice"}
	token := signEdDSAJWT(t, privateKey, claims)
	access := signEdDSAJWT(t, privateKey, map[string]any{"iss": server.URL, "sub": "gb_sub_1", "aud": []string{"graphit-broker", resource}, "iat": now.Unix(), "exp": now.Add(time.Hour).Unix(), "jti": "access-1", "client_id": "graphit-cli", "token_use": "access", "preferred_username": "alice"})
	profile, err := client.profileFromToken(context.Background(), Provider{Name: "broker", Type: ProviderBroker, Revision: 1,
		OIDC: &OIDCConfig{Issuer: server.URL, ClientID: "graphit-cli", UsernameClaim: "preferred_username", MCPAudience: "graphit-broker", MCPResource: resource}}, discovery,
		tokenResponse{AccessToken: access, IDToken: token, TokenType: "Bearer", ExpiresIn: 600}, "nonce")
	if err != nil || profile.Subject != "gb_sub_1" || profile.Username != "alice" {
		t.Fatalf("profile=%#v err=%v", profile, err)
	}
}

func TestBrokerIDTokenAudienceValidation(t *testing.T) {
	const clientID = "web-client"
	const brokerAudience = "graphit-broker"
	const resource = "https://broker.invalid/v1"
	for _, tc := range []struct {
		name   string
		claims map[string]any
		valid  bool
	}{
		{"single client audience", map[string]any{"aud": clientID}, true},
		{"broker multi audience", map[string]any{"aud": []any{brokerAudience, resource, clientID}, "azp": clientID}, true},
		{"missing client audience", map[string]any{"aud": []any{brokerAudience, resource}, "azp": clientID}, false},
		{"untrusted audience", map[string]any{"aud": []any{brokerAudience, "other-client", clientID}, "azp": clientID}, false},
		{"missing authorized party", map[string]any{"aud": []any{brokerAudience, clientID}}, false},
		{"wrong authorized party", map[string]any{"aud": []any{brokerAudience, clientID}, "azp": "other-client"}, false},
		{"wrong single authorized party", map[string]any{"aud": clientID, "azp": "other-client"}, false},
		{"duplicate audience", map[string]any{"aud": []any{clientID, clientID}, "azp": clientID}, false},
	} {
		t.Run(tc.name, func(t *testing.T) {
			if got := brokerIDTokenAudienceValid(tc.claims, clientID, brokerAudience, resource); got != tc.valid {
				t.Fatalf("audience valid=%v, want %v", got, tc.valid)
			}
		})
	}
}

func TestExchangeCodeClassifiesTokenValidationWithoutTokenValues(t *testing.T) {
	for _, test := range []struct {
		name, tokenKind, reason string
		mutate                  func(map[string]any, map[string]any)
		missingID               bool
		wrongSignature          bool
	}{
		{name: "missing token", tokenKind: "response", reason: "missing_token", missingID: true},
		{name: "ID token signature", tokenKind: "id_token", reason: "signature", wrongSignature: true},
		{name: "ID token issuer", tokenKind: "id_token", reason: "issuer", mutate: func(id, _ map[string]any) { id["iss"] = "https://other.invalid" }},
		{name: "ID token nonce", tokenKind: "id_token", reason: "nonce", mutate: func(id, _ map[string]any) { id["nonce"] = "different" }},
		{name: "ID token expired", tokenKind: "id_token", reason: "expired", mutate: func(id, _ map[string]any) { id["exp"] = time.Now().Add(-time.Minute).Unix() }},
		{name: "access token audience", tokenKind: "access_token", reason: "audience", mutate: func(_, access map[string]any) { access["aud"] = "other-audience" }},
		{name: "resource audience", tokenKind: "access_token", reason: "resource_audience", mutate: func(_, access map[string]any) { access["aud"] = "graphit-broker" }},
	} {
		t.Run(test.name, func(t *testing.T) {
			publicKey, privateKey, err := ed25519.GenerateKey(rand.Reader)
			if err != nil {
				t.Fatal(err)
			}
			now := time.Now().UTC()
			var server *httptest.Server
			server = httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				switch r.URL.Path {
				case "/token":
					id := map[string]any{"iss": server.URL, "sub": "gb_sub_1", "aud": "web-client", "iat": now.Unix(), "exp": now.Add(time.Hour).Unix(), "nonce": "nonce", "preferred_username": "alice"}
					access := map[string]any{"iss": server.URL, "sub": "gb_sub_1", "aud": []string{"graphit-broker", server.URL + "/v1"}, "iat": now.Unix(), "exp": now.Add(time.Hour).Unix(), "jti": "access-1", "client_id": "web-client", "token_use": "access"}
					if test.mutate != nil {
						test.mutate(id, access)
					}
					idToken := signEdDSAJWT(t, privateKey, id)
					if test.wrongSignature {
						_, otherKey, keyErr := ed25519.GenerateKey(rand.Reader)
						if keyErr != nil {
							t.Error(keyErr)
							return
						}
						idToken = signEdDSAJWT(t, otherKey, id)
					}
					if test.missingID {
						idToken = ""
					}
					writeJSON(t, w, map[string]any{"access_token": signEdDSAJWT(t, privateKey, access), "id_token": idToken, "token_type": "Bearer", "expires_in": 600})
				case "/jwks":
					writeJSON(t, w, map[string]any{"keys": []any{map[string]any{"kty": "OKP", "kid": "broker", "alg": "EdDSA", "use": "sig", "crv": "Ed25519", "x": base64.RawURLEncoding.EncodeToString(publicKey)}}})
				default:
					http.NotFound(w, r)
				}
			}))
			defer server.Close()
			client := &OIDCClient{HTTP: server.Client(), Now: func() time.Time { return now }}
			provider := Provider{Name: "broker", Type: ProviderBroker, OIDC: &OIDCConfig{Issuer: server.URL, ClientID: "web-client", UsernameClaim: "preferred_username", MCPAudience: "graphit-broker", MCPResource: server.URL + "/v1"}}
			_, err = client.ExchangeCode(context.Background(), provider, OIDCDiscovery{Issuer: server.URL, TokenEndpoint: server.URL + "/token", JWKSURI: server.URL + "/jwks"}, "private-code", "private-verifier", "https://ui.invalid/api/auth/callback", "nonce")
			var failure *CodeExchangeFailure
			if !errors.As(err, &failure) || failure.Stage != "token_validation" || failure.TokenKind != test.tokenKind || failure.ValidationReason != test.reason || failure.HTTPStatus != 0 || failure.OAuthCode != "" {
				t.Fatalf("diagnostics: failure=%#v err=%v", failure, err)
			}
		})
	}
}

func TestOIDCCallbackPageStatesAreSelfContained(t *testing.T) {
	tests := []struct {
		name       string
		success    bool
		expected   []string
		unexpected []string
	}{
		{name: "success", success: true, expected: []string{`class="success"`, "Sign-in received", "Secure sign-in"}, unexpected: []string{"Authentication not completed"}},
		{name: "error", success: false, expected: []string{`class="error"`, "Authentication not completed", "Sign-in interrupted"}, unexpected: []string{"Sign-in received"}},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			recorder := httptest.NewRecorder()
			writeOIDCCallbackPage(recorder, test.success)
			response := recorder.Result()
			defer response.Body.Close()
			body, err := io.ReadAll(response.Body)
			if err != nil {
				t.Fatal(err)
			}
			page := string(body)
			if response.Header.Get("Content-Type") != "text/html; charset=utf-8" || response.Header.Get("Referrer-Policy") != "no-referrer" ||
				response.Header.Get("X-Content-Type-Options") != "nosniff" {
				t.Fatalf("missing callback security headers: %#v", response.Header)
			}
			for _, expected := range append(test.expected,
				"@media (max-width: 430px)", "@media (prefers-color-scheme: dark)", "@media (prefers-reduced-motion: no-preference)",
				`<main class="shell" aria-labelledby="page-title">`, "No credentials shown") {
				if !strings.Contains(page, expected) {
					t.Errorf("callback page does not contain %q", expected)
				}
			}
			for _, unexpected := range append(test.unexpected, "https://", "http://", "<script", "<form") {
				if strings.Contains(page, unexpected) {
					t.Errorf("callback page unexpectedly contains %q", unexpected)
				}
			}
		})
	}
}

func TestOIDCCallbackPageUsesEscapedBuildBrand(t *testing.T) {
	originalDisplayName := brand.DisplayName
	t.Cleanup(func() { brand.DisplayName = originalDisplayName })
	brand.DisplayName = "Acme <Code>: private agent harness"

	recorder := httptest.NewRecorder()
	writeOIDCCallbackPage(recorder, true)
	page := recorder.Body.String()
	if !strings.Contains(page, "Acme &lt;Code&gt;") || strings.Contains(page, "private agent harness") || strings.Contains(page, "Acme <Code>") {
		t.Fatalf("callback page did not safely apply the short build brand: %s", page)
	}
}

func signEdDSAJWT(t *testing.T, key ed25519.PrivateKey, claims map[string]any) string {
	t.Helper()
	header, _ := json.Marshal(map[string]any{"alg": "EdDSA", "kid": "broker", "typ": "JWT"})
	payload, _ := json.Marshal(claims)
	signingInput := base64.RawURLEncoding.EncodeToString(header) + "." + base64.RawURLEncoding.EncodeToString(payload)
	signature := ed25519.Sign(key, []byte(signingInput))
	return signingInput + "." + base64.RawURLEncoding.EncodeToString(signature)
}

func cloneClaims(input map[string]any) map[string]any {
	output := make(map[string]any, len(input))
	for key, value := range input {
		output[key] = value
	}
	return output
}

func TestAuthorizationRequestUsesPKCEAndResource(t *testing.T) {
	client := NewOIDCClient()
	provider := Provider{Type: ProviderBroker, OIDC: &OIDCConfig{ClientID: "client", Scopes: []string{"profile"}, AuthParams: map[string]string{"prompt": "select_account"}, MCPResource: "res"}}
	req, err := client.AuthorizationRequest(provider, OIDCDiscovery{AuthorizationEndpoint: "https://id.example/auth"}, "http://127.0.0.1/callback")
	if err != nil {
		t.Fatal(err)
	}
	u, err := url.Parse(req.URL)
	if err != nil {
		t.Fatal(err)
	}
	q := u.Query()
	if q.Get("code_challenge_method") != "S256" || q.Has("audience") || q.Get("resource") != "res" || q.Get("prompt") != "select_account" || !strings.Contains(q.Get("scope"), "openid") {
		t.Fatalf("bad authorization URL: %s", req.URL)
	}
	if req.State == "" || req.Nonce == "" || req.Verifier == "" {
		t.Fatal("missing security parameters")
	}
}

func TestAuthorizationRequestCanOmitNonce(t *testing.T) {
	withoutNonce := false
	provider := Provider{Type: ProviderBroker, OIDC: &OIDCConfig{ClientID: "client", NonceEnabled: &withoutNonce}}
	req, err := NewOIDCClient().AuthorizationRequest(provider, OIDCDiscovery{AuthorizationEndpoint: "https://id.example/auth"}, "http://127.0.0.1/callback")
	if err != nil {
		t.Fatal(err)
	}
	u, err := url.Parse(req.URL)
	if err != nil {
		t.Fatal(err)
	}
	if _, sent := u.Query()["nonce"]; sent || req.Nonce != "" {
		t.Fatalf("nonce was sent despite being disabled: %#v", req)
	}
	if req.State == "" || req.Verifier == "" || u.Query().Get("code_challenge_method") != "S256" {
		t.Fatalf("authorization request lost state or PKCE: %#v", req)
	}
}

func TestOIDCClaimMappingsSupportJSONPathAndExactTopLevelClaims(t *testing.T) {
	claims := map[string]any{
		"organization":                     map[string]any{"id": "acme"},
		"authorization":                    map[string]any{"teams": []any{"platform", "security"}},
		"https://claims.example.com/teams": []any{"namespaced"},
		"organization.id":                  "literal",
		"members":                          []any{map[string]any{"active": true, "name": "alice"}, map[string]any{"active": false, "name": "bob"}},
	}
	organization, err := stringClaim(claims, "$.organization.id", true)
	if err != nil || organization != "acme" {
		t.Fatalf("organization=%q err=%v", organization, err)
	}
	literal, err := stringClaim(claims, "organization.id", true)
	if err != nil || literal != "literal" {
		t.Fatalf("literal=%q err=%v", literal, err)
	}
	teams, err := stringSliceClaim(claims, "$.authorization.teams")
	if err != nil || len(teams) != 2 || teams[0] != "platform" {
		t.Fatalf("teams=%v err=%v", teams, err)
	}
	filtered, err := stringClaim(claims, `$.members[?@.active == true].name`, true)
	if err != nil || filtered != "alice" {
		t.Fatalf("filtered=%q err=%v", filtered, err)
	}
	namespaced, err := stringSliceClaim(claims, "https://claims.example.com/teams")
	if err != nil || len(namespaced) != 1 || namespaced[0] != "namespaced" {
		t.Fatalf("namespaced=%v err=%v", namespaced, err)
	}
	delete(claims, "organization.id")
	if _, err := stringClaim(claims, "organization.id", true); err == nil {
		t.Fatal("implicit dotted traversal was accepted")
	}
	if _, err := stringClaim(claims, "$.members[*].name", true); err == nil || !strings.Contains(err.Error(), "exactly one") {
		t.Fatalf("ambiguous scalar selection: %v", err)
	}
	selectedTeams, err := stringSliceClaim(claims, "$.members[*].name")
	if err != nil || len(selectedTeams) != 2 || selectedTeams[0] != "alice" || selectedTeams[1] != "bob" {
		t.Fatalf("selected teams=%v err=%v", selectedTeams, err)
	}
	if _, err := stringSliceClaim(claims, "$.members[*].active"); err == nil {
		t.Fatal("non-string teams selection was accepted")
	}
	if _, err := stringClaim(claims, "$.missing", true); err == nil {
		t.Fatal("missing required claim was accepted")
	}
	if optional, err := stringClaim(claims, "$.missing", false); err != nil || optional != "" {
		t.Fatalf("missing optional claim=%q err=%v", optional, err)
	}
}

func writeJSON(t *testing.T, w http.ResponseWriter, v any) {
	t.Helper()
	w.Header().Set("Content-Type", "application/json")
	if err := json.NewEncoder(w).Encode(v); err != nil {
		t.Error(err)
	}
}
