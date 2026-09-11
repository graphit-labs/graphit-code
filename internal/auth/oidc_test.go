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
	"fmt"
	"math/big"
	"net/http"
	"net/http/httptest"
	"net/url"
	"strings"
	"testing"
	"time"
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
	claims := map[string]any{"iss": server.URL, "sub": "gb_sub_1", "aud": "graphit-cli", "exp": now.Add(time.Hour).Unix(), "nonce": "nonce", "preferred_username": "alice"}
	token := signEdDSAJWT(t, privateKey, claims)
	profile, err := client.profileFromToken(context.Background(), Provider{Name: "broker", Type: ProviderBroker, Revision: 1,
		OIDC: &OIDCConfig{Issuer: server.URL, ClientID: "graphit-cli", UsernameClaim: "preferred_username"}}, discovery,
		tokenResponse{AccessToken: "opaque", IDToken: token, ExpiresIn: 600}, "nonce")
	if err != nil || profile.Subject != "gb_sub_1" || profile.Username != "alice" {
		t.Fatalf("profile=%#v err=%v", profile, err)
	}
}

func TestDirectOIDCLoginInteractiveEndToEnd(t *testing.T) {
	key, err := rsa.GenerateKey(rand.Reader, 2048)
	if err != nil {
		t.Fatal(err)
	}
	now := time.Date(2026, 9, 9, 18, 0, 0, 0, time.UTC)
	var issuer *httptest.Server
	var nonce, challenge, redirectURI string
	issuer = httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/.well-known/openid-configuration":
			writeJSON(t, w, map[string]any{"issuer": issuer.URL, "authorization_endpoint": issuer.URL + "/authorize", "token_endpoint": issuer.URL + "/token", "jwks_uri": issuer.URL + "/jwks"})
		case "/jwks":
			writeJSON(t, w, rsaJWKSForBrokerTest(key))
		case "/token":
			if err := r.ParseForm(); err != nil {
				t.Error(err)
				return
			}
			if r.Form.Get("grant_type") != "authorization_code" || r.Form.Get("code") != "direct-code" ||
				r.Form.Get("client_id") != "direct-client" || r.Form.Get("redirect_uri") != redirectURI ||
				sha256Sum(r.Form.Get("code_verifier")) != challenge {
				t.Errorf("invalid direct OIDC code exchange: %#v", r.Form)
				http.Error(w, "invalid exchange", http.StatusBadRequest)
				return
			}
			claims := map[string]any{"iss": issuer.URL, "sub": "direct-subject", "aud": "direct-client",
				"exp": now.Add(time.Hour).Unix(), "nonce": nonce, "preferred_username": "alice",
				"organization": "acme", "groups": []string{"platform", "security"}}
			writeJSON(t, w, map[string]any{"access_token": "direct-access", "refresh_token": "direct-refresh",
				"id_token": signJWT(t, key, claims), "token_type": "Bearer", "expires_in": 600})
		default:
			http.NotFound(w, r)
		}
	}))
	defer issuer.Close()

	provider := Provider{Name: "direct", Type: ProviderOIDC, Revision: 7, OIDC: &OIDCConfig{
		Issuer: issuer.URL, ClientID: "direct-client", TokenAuthMethod: "none",
		Scopes: []string{"openid", "profile", "offline_access"}, UsernameClaim: "preferred_username",
		OrganizationClaim: "organization", TeamsClaim: "groups",
	}}
	client := &OIDCClient{HTTP: issuer.Client(), Now: func() time.Time { return now }}
	profile, err := client.LoginInteractive(context.Background(), provider, func(target string) error {
		request, parseErr := url.Parse(target)
		if parseErr != nil {
			return parseErr
		}
		if request.Path != "/authorize" || request.Query().Get("response_type") != "code" ||
			request.Query().Get("client_id") != "direct-client" || request.Query().Get("code_challenge_method") != "S256" {
			t.Errorf("invalid direct OIDC authorization request: %s", target)
		}
		nonce = request.Query().Get("nonce")
		challenge = request.Query().Get("code_challenge")
		redirectURI = request.Query().Get("redirect_uri")
		callback := redirectURI + "?code=direct-code&state=" + url.QueryEscape(request.Query().Get("state"))
		go func() {
			response, requestErr := http.Get(callback)
			if requestErr == nil {
				_ = response.Body.Close()
			}
		}()
		return nil
	})
	if err != nil {
		t.Fatal(err)
	}
	if nonce == "" || challenge == "" || redirectURI == "" || profile.Provider != "direct" ||
		profile.ProviderRevision != 7 || profile.Issuer != issuer.URL || profile.Subject != "direct-subject" ||
		profile.Username != "alice" || profile.Organization != "acme" || strings.Join(profile.Teams, ",") != "platform,security" ||
		profile.OIDC == nil || profile.OIDC.AccessToken != "direct-access" || profile.OIDC.RefreshToken != "direct-refresh" {
		t.Fatalf("direct OIDC profile=%#v nonce=%q challenge=%q redirect=%q", profile, nonce, challenge, redirectURI)
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

func TestOIDCDiscoveryRefreshAndVerifiedClaimMapping(t *testing.T) {
	key, err := rsa.GenerateKey(rand.Reader, 2048)
	if err != nil {
		t.Fatal(err)
	}
	now := time.Unix(2_000_000_000, 0)
	var server *httptest.Server
	server = httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/.well-known/openid-configuration":
			writeJSON(t, w, map[string]any{"issuer": server.URL, "authorization_endpoint": server.URL + "/authorize", "token_endpoint": server.URL + "/token", "jwks_uri": server.URL + "/jwks"})
		case "/jwks":
			writeJSON(t, w, map[string]any{"keys": []any{map[string]any{"kty": "RSA", "kid": "test", "alg": "RS256", "n": base64.RawURLEncoding.EncodeToString(key.N.Bytes()), "e": base64.RawURLEncoding.EncodeToString(big.NewInt(int64(key.E)).Bytes())}}})
		case "/token":
			if err := r.ParseForm(); err != nil {
				t.Error(err)
			}
			if r.Form.Get("grant_type") != "refresh_token" || (r.Form.Get("refresh_token") != "refresh" && r.Form.Get("refresh_token") != "refresh-no-id") {
				http.Error(w, "bad refresh", 400)
				return
			}
			claims := map[string]any{"iss": server.URL, "sub": "subject-1", "aud": "client", "exp": now.Add(time.Hour).Unix(), "preferred_username": "alice", "org": "acme", "groups": []string{"dev", "ops"}}
			response := map[string]any{"access_token": "new-access", "refresh_token": "rotated", "token_type": "Bearer", "expires_in": 3600}
			if r.Form.Get("refresh_token") == "refresh" {
				response["id_token"] = signJWT(t, key, claims)
			}
			writeJSON(t, w, response)
		default:
			http.NotFound(w, r)
		}
	}))
	defer server.Close()
	client := NewOIDCClient()
	client.Now = func() time.Time { return now }
	client.HTTP = server.Client()
	provider := Provider{Name: "corp", Type: ProviderOIDC, Revision: 3, OIDC: &OIDCConfig{Issuer: server.URL, ClientID: "client", UsernameClaim: "preferred_username", OrganizationClaim: "org", TeamsClaim: "groups"}}
	profile, err := client.Refresh(context.Background(), provider, Profile{Issuer: server.URL, Subject: "subject-1", Username: "alice", OIDC: &OIDCSession{RefreshToken: "refresh", IDToken: "old"}})
	if err != nil {
		t.Fatal(err)
	}
	if profile.Username != "alice" || profile.Organization != "acme" || strings.Join(profile.Teams, ",") != "dev,ops" {
		t.Fatalf("claims not mapped: %#v", profile)
	}
	if profile.Issuer != server.URL || profile.Subject != "subject-1" || profile.OIDC.AccessToken != "new-access" {
		t.Fatalf("identity/session mismatch: %#v", profile)
	}
	preserved, err := client.Refresh(context.Background(), provider, Profile{Name: "alice", Provider: "corp", ProviderRevision: 3, Issuer: server.URL, Subject: "subject-1", Username: "alice", OIDC: &OIDCSession{RefreshToken: "refresh-no-id", IDToken: "previous-verified"}})
	if err != nil {
		t.Fatal(err)
	}
	if preserved.Subject != "subject-1" || preserved.Username != "alice" || preserved.OIDC.IDToken != "previous-verified" || preserved.OIDC.AccessToken != "new-access" {
		t.Fatalf("verified identity was not preserved: %#v", preserved)
	}
}

func TestOIDCFailsClosedOnInvalidAudience(t *testing.T) {
	key, _ := rsa.GenerateKey(rand.Reader, 2048)
	now := time.Unix(2_000_000_000, 0)
	var server *httptest.Server
	server = httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/.well-known/openid-configuration":
			writeJSON(t, w, map[string]any{"issuer": server.URL, "authorization_endpoint": server.URL + "/auth", "token_endpoint": server.URL + "/token", "jwks_uri": server.URL + "/jwks"})
		case "/jwks":
			writeJSON(t, w, map[string]any{"keys": []any{map[string]any{"kty": "RSA", "kid": "test", "alg": "RS256", "n": base64.RawURLEncoding.EncodeToString(key.N.Bytes()), "e": "AQAB"}}})
		}
	}))
	defer server.Close()
	client := NewOIDCClient()
	client.Now = func() time.Time { return now }
	client.HTTP = server.Client()
	discovery, err := client.Discovery(context.Background(), server.URL)
	if err != nil {
		t.Fatal(err)
	}
	token := signJWT(t, key, map[string]any{"iss": server.URL, "sub": "s", "aud": "other", "exp": now.Add(time.Hour).Unix(), "name": "alice"})
	_, err = client.profileFromToken(context.Background(), Provider{Name: "p", Type: ProviderOIDC, Revision: 1, OIDC: &OIDCConfig{Issuer: server.URL, ClientID: "client", UsernameClaim: "name"}}, discovery, tokenResponse{AccessToken: "a", IDToken: token, ExpiresIn: 60}, "")
	if err == nil || !strings.Contains(err.Error(), "audience") {
		t.Fatalf("expected audience rejection, got %v", err)
	}
}

func TestOIDCFailsClosedOnInvalidIssuerSignatureAndNonce(t *testing.T) {
	key, _ := rsa.GenerateKey(rand.Reader, 2048)
	now := time.Now().UTC()
	var server *httptest.Server
	server = httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/.well-known/openid-configuration":
			writeJSON(t, w, map[string]any{"issuer": server.URL, "authorization_endpoint": server.URL + "/auth", "token_endpoint": server.URL + "/token", "jwks_uri": server.URL + "/jwks"})
		case "/jwks":
			writeJSON(t, w, rsaJWKSForBrokerTest(key))
		}
	}))
	defer server.Close()
	client := &OIDCClient{HTTP: server.Client(), Now: func() time.Time { return now }}
	discovery, err := client.Discovery(context.Background(), server.URL)
	if err != nil {
		t.Fatal(err)
	}
	provider := Provider{Type: ProviderOIDC, OIDC: &OIDCConfig{Issuer: server.URL, ClientID: "client", UsernameClaim: "preferred_username"}}
	validClaims := map[string]any{"iss": server.URL, "sub": "subject", "aud": "client", "exp": now.Add(time.Hour).Unix(), "nonce": "expected", "preferred_username": "alice"}
	wrongIssuer := cloneClaims(validClaims)
	wrongIssuer["iss"] = "https://attacker.example"
	wrongNonce := cloneClaims(validClaims)
	wrongNonce["nonce"] = "attacker"
	valid := signJWT(t, key, validClaims)
	parts := strings.Split(valid, ".")
	parts[2] = base64.RawURLEncoding.EncodeToString(make([]byte, 256))
	for name, token := range map[string]string{
		"issuer":    signJWT(t, key, wrongIssuer),
		"nonce":     signJWT(t, key, wrongNonce),
		"signature": strings.Join(parts, "."),
	} {
		t.Run(name, func(t *testing.T) {
			if _, err := client.profileFromToken(context.Background(), provider, discovery, tokenResponse{AccessToken: "opaque", IDToken: token, ExpiresIn: 60}, "expected"); err == nil {
				t.Fatalf("invalid %s accepted", name)
			}
		})
	}
}

func cloneClaims(input map[string]any) map[string]any {
	output := make(map[string]any, len(input))
	for key, value := range input {
		output[key] = value
	}
	return output
}

func TestVerifyAccessTokenValidatesAndMapsOnlySignedClaims(t *testing.T) {
	key, err := rsa.GenerateKey(rand.Reader, 2048)
	if err != nil {
		t.Fatal(err)
	}
	now := time.Unix(2_000_000_000, 0)
	var server *httptest.Server
	server = httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/.well-known/openid-configuration":
			writeJSON(t, w, map[string]any{"issuer": server.URL, "authorization_endpoint": server.URL + "/auth", "token_endpoint": server.URL + "/token", "jwks_uri": server.URL + "/jwks"})
		case "/jwks":
			writeJSON(t, w, map[string]any{"keys": []any{map[string]any{"kty": "RSA", "kid": "test", "alg": "RS256", "n": base64.RawURLEncoding.EncodeToString(key.N.Bytes()), "e": "AQAB"}}})
		default:
			http.NotFound(w, r)
		}
	}))
	defer server.Close()
	client := NewOIDCClient()
	client.Now = func() time.Time { return now }
	client.HTTP = server.Client()
	provider := Provider{Name: "corp", Type: ProviderOIDC, OIDC: &OIDCConfig{Issuer: server.URL, ClientID: "client", UsernameClaim: "preferred_username", OrganizationClaim: "org", TeamsClaim: "groups"}}
	token := signJWT(t, key, map[string]any{
		"iss": server.URL, "sub": "subject-1", "aud": []string{"graphit-mcp"},
		"exp": now.Add(time.Hour).Unix(), "nbf": now.Add(-time.Minute).Unix(),
		"preferred_username": "alice", "org": "acme", "groups": []string{"platform", "security"},
	})
	identity, err := client.VerifyAccessToken(context.Background(), provider, token, "graphit-mcp")
	if err != nil {
		t.Fatal(err)
	}
	if identity.Issuer != server.URL || identity.Subject != "subject-1" || identity.Username != "alice" || identity.Organization != "acme" || strings.Join(identity.Teams, ",") != "platform,security" {
		t.Fatalf("identity=%#v", identity)
	}
	if _, err := client.VerifyAccessToken(context.Background(), provider, token, "other-api"); err == nil || !strings.Contains(err.Error(), "audience") {
		t.Fatalf("wrong audience was not rejected: %v", err)
	}
	future := signJWT(t, key, map[string]any{"iss": server.URL, "sub": "subject-1", "aud": "graphit-mcp", "exp": now.Add(time.Hour).Unix(), "nbf": now.Add(time.Minute).Unix(), "preferred_username": "alice"})
	if _, err := client.VerifyAccessToken(context.Background(), provider, future, "graphit-mcp"); err == nil || !strings.Contains(err.Error(), "not active") {
		t.Fatalf("future token was not rejected: %v", err)
	}
}

func TestAuthorizationRequestUsesPKCEAudienceAndResource(t *testing.T) {
	client := NewOIDCClient()
	provider := Provider{Type: ProviderOIDC, OIDC: &OIDCConfig{ClientID: "client", Scopes: []string{"profile"}, AuthParams: map[string]string{"prompt": "select_account"}}, MCP: MCPConfig{Audience: "aud", Resource: "res"}}
	req, err := client.AuthorizationRequest(provider, OIDCDiscovery{AuthorizationEndpoint: "https://id.example/auth"}, "http://127.0.0.1/callback")
	if err != nil {
		t.Fatal(err)
	}
	u, err := url.Parse(req.URL)
	if err != nil {
		t.Fatal(err)
	}
	q := u.Query()
	if q.Get("code_challenge_method") != "S256" || q.Get("audience") != "aud" || q.Get("resource") != "res" || q.Get("prompt") != "select_account" || !strings.Contains(q.Get("scope"), "openid") {
		t.Fatalf("bad authorization URL: %s", req.URL)
	}
	if req.State == "" || req.Nonce == "" || req.Verifier == "" {
		t.Fatal("missing security parameters")
	}
}

func TestAuthorizationRequestKeepsMCPAudienceForBrokerTokenExchange(t *testing.T) {
	client := NewOIDCClient()
	provider := Provider{
		Type: ProviderOIDC,
		OIDC: &OIDCConfig{ClientID: "client"},
		MCP:  MCPConfig{Audience: "graphit-mcp", Resource: "https://mcp.example"},
		Broker: &BrokerConfig{
			Audience:      "graphit-broker",
			Resource:      "https://broker.example",
			TokenStrategy: "token-exchange",
		},
	}
	req, err := client.AuthorizationRequest(provider, OIDCDiscovery{AuthorizationEndpoint: "https://id.example/auth"}, "http://127.0.0.1/callback")
	if err != nil {
		t.Fatal(err)
	}
	u, err := url.Parse(req.URL)
	if err != nil {
		t.Fatal(err)
	}
	if got := u.Query().Get("audience"); got != "graphit-mcp" {
		t.Fatalf("audience=%q, want graphit-mcp", got)
	}
	if got := u.Query().Get("resource"); got != "https://mcp.example" {
		t.Fatalf("resource=%q, want MCP resource", got)
	}
}

func TestProviderValidatesBrokerTokenAudienceStrategy(t *testing.T) {
	base := Provider{
		Name: "corp", Type: ProviderOIDC,
		OIDC:   &OIDCConfig{Issuer: "https://issuer.example", ClientID: "client", UsernameClaim: "name"},
		MCP:    MCPConfig{Audience: "graphit-mcp"},
		Broker: &BrokerConfig{Endpoint: "https://broker.example", Audience: "graphit-broker"},
		AI:     AIConfig{Embedding: AIServiceConfig{Mode: ServiceBroker}, Rerank: AIServiceConfig{Mode: ServiceBroker}},
	}
	if err := ValidateProvider(base); err == nil || !strings.Contains(err.Error(), "audiences must match") {
		t.Fatalf("expected relay audience mismatch rejection, got %v", err)
	}
	base.Broker.Audience = "graphit-mcp"
	base.MCP.Resource = "https://mcp.example"
	base.Broker.Resource = "https://broker.example"
	if err := ValidateProvider(base); err == nil || !strings.Contains(err.Error(), "resources must match") {
		t.Fatalf("expected relay resource mismatch rejection, got %v", err)
	}
	base.Broker.TokenStrategy = "token-exchange"
	base.Broker.Audience = "graphit-broker"
	if err := ValidateProvider(base); err != nil {
		t.Fatalf("token exchange should allow separate audiences: %v", err)
	}
	base.MCP.Audience = ""
	if err := ValidateProvider(base); err == nil || !strings.Contains(err.Error(), "MCP audience") {
		t.Fatalf("expected missing MCP audience rejection, got %v", err)
	}
}

func TestProviderRejectsReservedOIDCAuthorizationParameters(t *testing.T) {
	err := ValidateProvider(Provider{Name: "corp", Type: ProviderOIDC, OIDC: &OIDCConfig{Issuer: "https://issuer.example", ClientID: "client", UsernameClaim: "name", AuthParams: map[string]string{"state": "attacker"}}})
	if err == nil || !strings.Contains(err.Error(), "reserved") {
		t.Fatalf("expected reserved-parameter rejection, got %v", err)
	}
}

func TestOIDCClaimMappingsSupportNestedPathsAndExactNamespacedClaims(t *testing.T) {
	claims := map[string]any{
		"organization":                     map[string]any{"id": "acme"},
		"authorization":                    map[string]any{"teams": []any{"platform", "security"}},
		"https://claims.example.com/teams": []any{"namespaced"},
	}
	organization, err := stringClaim(claims, "organization.id", true)
	if err != nil || organization != "acme" {
		t.Fatalf("organization=%q err=%v", organization, err)
	}
	teams, err := stringSliceClaim(claims, "authorization.teams")
	if err != nil || len(teams) != 2 || teams[0] != "platform" {
		t.Fatalf("teams=%v err=%v", teams, err)
	}
	namespaced, err := stringSliceClaim(claims, "https://claims.example.com/teams")
	if err != nil || len(namespaced) != 1 || namespaced[0] != "namespaced" {
		t.Fatalf("namespaced=%v err=%v", namespaced, err)
	}
}

func TestOIDCTokenClientSecretBasic(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		username, password, ok := r.BasicAuth()
		if !ok || username != "client" || password != "secret" {
			http.Error(w, "missing client authentication", http.StatusUnauthorized)
			return
		}
		if err := r.ParseForm(); err != nil {
			t.Error(err)
			return
		}
		if r.Form.Get("client_secret") != "" {
			t.Error("client secret was duplicated in the request body")
		}
		writeJSON(t, w, map[string]any{"access_token": "access", "id_token": "id", "token_type": "Bearer", "expires_in": 60})
	}))
	defer server.Close()
	client := NewOIDCClient()
	client.HTTP = server.Client()
	_, err := client.token(context.Background(), server.URL, url.Values{"grant_type": {"authorization_code"}}, &OIDCConfig{ClientID: "client", ClientSecret: "secret", TokenAuthMethod: "client_secret_basic"})
	if err != nil {
		t.Fatal(err)
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

func TestAWSSTSExchange(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		body := make([]byte, r.ContentLength)
		_, _ = r.Body.Read(body)
		values, _ := url.ParseQuery(string(body))
		if values.Get("WebIdentityToken") != "id-token" {
			t.Errorf("token = %q", values.Get("WebIdentityToken"))
		}
		w.Header().Set("Content-Type", "text/xml")
		fmt.Fprint(w, `<AssumeRoleWithWebIdentityResponse xmlns="https://sts.amazonaws.com/doc/2011-06-15/"><AssumeRoleWithWebIdentityResult><Credentials><AccessKeyId>AKIA_TEST</AccessKeyId><SecretAccessKey>secret</SecretAccessKey><SessionToken>session</SessionToken><Expiration>2030-01-01T00:00:00Z</Expiration></Credentials></AssumeRoleWithWebIdentityResult><ResponseMetadata><RequestId>request</RequestId></ResponseMetadata></AssumeRoleWithWebIdentityResponse>`)
	}))
	defer server.Close()
	provider := Provider{Name: "corp", S3: S3Config{Region: "us-east-1"}, STS: &STSConfig{Endpoint: server.URL, RoleARN: "arn:aws:iam::123456789012:role/graphit"}}
	creds, err := (AWSSTSExchanger{}).Exchange(context.Background(), provider, Profile{Name: "alice", OIDC: &OIDCSession{IDToken: "id-token"}})
	if err != nil {
		t.Fatal(err)
	}
	if creds.AccessKeyID != "AKIA_TEST" || creds.SecretAccessKey != "secret" || creds.SessionToken != "session" {
		t.Fatalf("bad credentials: %#v", creds)
	}
}
