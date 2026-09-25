package auth

import (
	"context"
	"crypto"
	"crypto/ed25519"
	"crypto/rand"
	"crypto/rsa"
	"crypto/sha256"
	"encoding/base64"
	"encoding/json"
	"io"
	"math/big"
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
	claims := map[string]any{"iss": server.URL, "sub": "gb_sub_1", "aud": "graphit-cli", "iat": now.Unix(), "exp": now.Add(time.Hour).Unix(), "nonce": "nonce", "preferred_username": "alice"}
	token := signEdDSAJWT(t, privateKey, claims)
	access := signEdDSAJWT(t, privateKey, map[string]any{"iss": server.URL, "sub": "gb_sub_1", "aud": "graphit-broker", "iat": now.Unix(), "exp": now.Add(time.Hour).Unix(), "jti": "access-1", "client_id": "graphit-cli", "token_use": "access", "preferred_username": "alice"})
	profile, err := client.profileFromToken(context.Background(), Provider{Name: "broker", Type: ProviderBroker, Revision: 1,
		OIDC: &OIDCConfig{Issuer: server.URL, ClientID: "graphit-cli", UsernameClaim: "preferred_username", MCPAudience: "graphit-broker"}}, discovery,
		tokenResponse{AccessToken: access, IDToken: token, TokenType: "Bearer", ExpiresIn: 600}, "nonce")
	if err != nil || profile.Subject != "gb_sub_1" || profile.Username != "alice" {
		t.Fatalf("profile=%#v err=%v", profile, err)
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

func rsaJWKSForBrokerTest(key *rsa.PrivateKey) map[string]any {
	return map[string]any{"keys": []any{map[string]any{"kty": "RSA", "kid": "test", "alg": "RS256",
		"n": base64.RawURLEncoding.EncodeToString(key.N.Bytes()), "e": base64.RawURLEncoding.EncodeToString(big.NewInt(int64(key.E)).Bytes())}}}
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

func signJWT(t *testing.T, key *rsa.PrivateKey, claims map[string]any) string {
	t.Helper()
	header, _ := json.Marshal(map[string]any{"alg": "RS256", "kid": "test", "typ": "JWT"})
	payload, _ := json.Marshal(claims)
	unsigned := base64.RawURLEncoding.EncodeToString(header) + "." + base64.RawURLEncoding.EncodeToString(payload)
	digest := sha256.Sum256([]byte(unsigned))
	sig, err := rsa.SignPKCS1v15(rand.Reader, key, crypto.SHA256, digest[:])
	if err != nil {
		t.Fatal(err)
	}
	return unsigned + "." + base64.RawURLEncoding.EncodeToString(sig)
}
func writeJSON(t *testing.T, w http.ResponseWriter, v any) {
	t.Helper()
	w.Header().Set("Content-Type", "application/json")
	if err := json.NewEncoder(w).Encode(v); err != nil {
		t.Error(err)
	}
}
