package auth

import (
	"context"
	"crypto/ed25519"
	"crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"net/url"
	"strings"
	"testing"
	"time"
)

func TestBrokerLoginAndAccessTokenVerificationUseStandardOIDCAndJWKS(t *testing.T) {
	_, key, err := ed25519.GenerateKey(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	fixed := time.Date(2026, 9, 9, 12, 0, 0, 0, time.UTC)
	var server *httptest.Server
	var nonce, challenge string
	userinfoCalls := 0
	userinfoSubject := "gb_sub_1"
	var allowedAccess string
	revoked := false
	server = httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/.well-known/graphit-broker":
			writeJSON(t, w, brokerDiscoveryForTest(server.URL))
		case "/.well-known/openid-configuration":
			writeJSON(t, w, oidcDiscoveryForBrokerTest(server.URL))
		case "/jwks":
			writeJSON(t, w, brokerJWKSForTest(key))
		case "/token":
			if err := r.ParseForm(); err != nil {
				t.Error(err)
			}
			claims := map[string]any{"iss": server.URL, "sub": "gb_sub_1", "aud": "graphit-cli", "iat": fixed.Unix(), "exp": fixed.Add(time.Hour).Unix(),
				"preferred_username": "alice", "organization": "acme", "groups": []string{"platform"}}
			switch r.Form.Get("grant_type") {
			case "authorization_code":
				digest := sha256Sum(r.Form.Get("code_verifier"))
				if r.Form.Get("code") != "broker-code" || digest != challenge || r.Form.Get("redirect_uri") == "" {
					t.Fatalf("invalid code exchange: %#v", r.Form)
				}
				claims["nonce"] = nonce
				writeStandardBrokerToken(t, w, key, claims, signBrokerJWT(t, key, brokerAccessClaimsForTest(server.URL, fixed, "access-1")), "refresh-1")
			case "refresh_token":
				if r.Form.Get("refresh_token") != "refresh-1" {
					t.Fatalf("refresh token=%q", r.Form.Get("refresh_token"))
				}
				writeStandardBrokerToken(t, w, key, claims, signBrokerJWT(t, key, brokerAccessClaimsForTest(server.URL, fixed, "access-2")), "refresh-2")
			default:
				http.Error(w, "unexpected grant", http.StatusBadRequest)
			}
		case "/userinfo":
			userinfoCalls++
			if revoked || r.Header.Get("Authorization") != "Bearer "+allowedAccess {
				http.Error(w, "invalid token", http.StatusUnauthorized)
				return
			}
			writeJSON(t, w, map[string]any{"sub": userinfoSubject})
		default:
			http.NotFound(w, r)
		}
	}))
	defer server.Close()

	provider := brokerProviderForTest(server.URL)
	provider.Broker.MCPResource = "https://graphit.example.com/mcp"
	resolved, err := BrokerOIDCProvider(context.Background(), provider, server.Client())
	if err != nil {
		t.Fatal(err)
	}
	if resolved.OIDC == nil || resolved.OIDC.MCPAudience != "graphit-broker" {
		t.Fatalf("resolved Broker access-token audience=%#v", resolved.OIDC)
	}
	client := &OIDCClient{HTTP: server.Client(), Now: func() time.Time { return fixed }}
	var openedURL string
	profile, err := client.LoginInteractive(context.Background(), resolved, func(target string) error {
		openedURL = target
		parsed, parseErr := url.Parse(target)
		if parseErr != nil {
			return parseErr
		}
		nonce = parsed.Query().Get("nonce")
		challenge = parsed.Query().Get("code_challenge")
		callback := parsed.Query().Get("redirect_uri") + "?code=broker-code&state=" + url.QueryEscape(parsed.Query().Get("state"))
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
	if !strings.HasPrefix(openedURL, server.URL+"/authorize?") || profile.Username != "alice" || profile.Subject != "gb_sub_1" || profile.Issuer != server.URL || profile.OIDC == nil || profile.OIDC.AccessToken == "" || profile.OIDC.RefreshToken != "refresh-1" {
		t.Fatalf("opened=%q profile=%#v", openedURL, profile)
	}
	allowedAccess = profile.OIDC.AccessToken
	refreshed, err := client.Refresh(context.Background(), resolved, profile)
	if err != nil || refreshed.OIDC.AccessToken == "" || refreshed.OIDC.AccessToken == profile.OIDC.AccessToken || refreshed.OIDC.RefreshToken != "refresh-2" || refreshed.Subject != profile.Subject {
		t.Fatalf("refreshed=%#v err=%v", refreshed, err)
	}
	verifier := &ProviderAccessTokenVerifier{OIDC: client}
	identity, err := verifier.VerifyAccessToken(context.Background(), provider, profile.OIDC.AccessToken, []string{provider.Broker.MCPResource})
	if err != nil || identity.Issuer != server.URL || identity.Username != "alice" || identity.Subject != "gb_sub_1" {
		t.Fatalf("verified broker identity=%#v err=%v", identity, err)
	}
	revoked = true
	if _, err := verifier.VerifyAccessToken(context.Background(), provider, allowedAccess, []string{provider.Broker.MCPResource}); err == nil {
		t.Fatal("revoked Broker access token was accepted")
	}
	revoked = false
	userinfoSubject = "another-person"
	if _, err := verifier.VerifyAccessToken(context.Background(), provider, allowedAccess, []string{provider.Broker.MCPResource}); err == nil {
		t.Fatal("Broker UserInfo subject mismatch was accepted")
	}
	userinfoSubject = "gb_sub_1"

	// A client registered with the broker after this daemon started carries a client_id the
	// daemon has never seen. The broker is what attests which clients exist, so such a token
	// must be accepted on its own merits; every other check stays in force.
	registeredClientClaims := brokerAccessClaimsForTest(server.URL, fixed, "registered-client")
	registeredClientClaims["client_id"] = "dynamically-registered-client"
	registeredToken := signBrokerJWT(t, key, registeredClientClaims)
	allowedAccess = registeredToken
	registeredIdentity, err := verifier.VerifyAccessToken(context.Background(), provider, registeredToken, []string{provider.Broker.MCPResource})
	if err != nil || registeredIdentity.Username != "alice" || registeredIdentity.Subject != "gb_sub_1" {
		t.Fatalf("token from a registered client was refused: identity=%#v err=%v", registeredIdentity, err)
	}
	allowedAccess = profile.OIDC.AccessToken

	// No product-specific scope is required: a token's reach is decided by its audience, so
	// one carrying only standard OIDC scopes is as valid as any other. Demanding a scope name
	// here would narrow which authorization servers can serve this endpoint.
	plainScopeClaims := brokerAccessClaimsForTest(server.URL, fixed, "plain-scope")
	plainScopeClaims["scope"] = "openid profile"
	plainScopeToken := signBrokerJWT(t, key, plainScopeClaims)
	allowedAccess = plainScopeToken
	plainScopeIdentity, err := verifier.VerifyAccessToken(context.Background(), provider, plainScopeToken, []string{provider.Broker.MCPResource})
	if err != nil || plainScopeIdentity.Username != "alice" {
		t.Fatalf("token without a product scope was refused: identity=%#v err=%v", plainScopeIdentity, err)
	}
	allowedAccess = profile.OIDC.AccessToken

	invalid := []struct {
		name   string
		mutate func(map[string]any)
	}{
		{name: "issuer", mutate: func(claims map[string]any) { claims["iss"] = "https://other.example" }},
		{name: "audience", mutate: func(claims map[string]any) { claims["aud"] = "other-api" }},
		{name: "broker audience only", mutate: func(claims map[string]any) { claims["aud"] = "graphit-broker" }},
		{name: "other MCP resource", mutate: func(claims map[string]any) {
			claims["aud"] = []string{"graphit-broker", "https://other.example.com/mcp"}
		}},
		{name: "expired", mutate: func(claims map[string]any) { claims["exp"] = fixed.Add(-time.Minute).Unix() }},
		{name: "username", mutate: func(claims map[string]any) { delete(claims, "preferred_username") }},
	}
	for _, test := range invalid {
		t.Run("reject "+test.name, func(t *testing.T) {
			claims := brokerAccessClaimsForTest(server.URL, fixed, "invalid-"+test.name)
			test.mutate(claims)
			if _, err := verifier.VerifyAccessToken(context.Background(), provider, signBrokerJWT(t, key, claims), []string{provider.Broker.MCPResource}); err == nil {
				t.Fatal("invalid Broker access token was accepted")
			}
		})
	}
	if _, err := verifier.VerifyAccessToken(context.Background(), provider, tamperBrokerJWT(profile.OIDC.AccessToken), []string{provider.Broker.MCPResource}); err == nil {
		t.Fatal("Broker access token with tampered signature was accepted")
	}
	// One accepted token, one revoked check, one mismatched UserInfo subject, one token from a
	// registered client, and one carrying only standard OIDC scopes. The rejections above never reach userinfo because
	// they fail JWT validation first.
	if userinfoCalls != 5 {
		t.Fatalf("Broker access-token verification called userinfo %d times, want 5", userinfoCalls)
	}
}

func TestBrokerOIDCRejectsMissingOrInconsistentAccessTokenAudience(t *testing.T) {
	for _, test := range []struct {
		name   string
		mutate func(map[string]any)
	}{
		{name: "missing", mutate: func(authentication map[string]any) { delete(authentication, "access_token_audience") }},
		{name: "not advertised", mutate: func(authentication map[string]any) { authentication["access_token_audience"] = "other-api" }},
	} {
		t.Run(test.name, func(t *testing.T) {
			var broker *httptest.Server
			broker = httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				switch r.URL.Path {
				case "/.well-known/graphit-broker":
					document := brokerDiscoveryForTest(broker.URL)
					test.mutate(document["authentication"].(map[string]any))
					writeJSON(t, w, document)
				default:
					http.NotFound(w, r)
				}
			}))
			defer broker.Close()
			if _, err := BrokerOIDCProvider(context.Background(), brokerProviderForTest(broker.URL), broker.Client()); err == nil || !strings.Contains(err.Error(), "access token audience") {
				t.Fatalf("invalid access-token audience accepted: %v", err)
			}
		})
	}
}

func TestBrokerLoginRejectsInvalidSignedTokens(t *testing.T) {
	_, key, err := ed25519.GenerateKey(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	now := time.Now().UTC()
	var server *httptest.Server
	server = httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/.well-known/openid-configuration":
			writeJSON(t, w, oidcDiscoveryForBrokerTest(server.URL))
		case "/jwks":
			writeJSON(t, w, brokerJWKSForTest(key))
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
	provider := Provider{Name: "broker", Type: ProviderBroker, OIDC: &OIDCConfig{Issuer: server.URL, ClientID: "graphit-cli", MCPAudience: "graphit-broker", UsernameClaim: "preferred_username"}}
	idBase := map[string]any{"iss": server.URL, "sub": "gb_sub_1", "aud": "graphit-cli", "iat": now.Unix(), "exp": now.Add(time.Hour).Unix(), "nonce": "sent-nonce", "preferred_username": "alice"}
	accessBase := brokerAccessClaimsForTest(server.URL, now, "access-1")
	makeToken := func(id, access map[string]any) tokenResponse {
		return tokenResponse{IDToken: signBrokerJWT(t, key, id), AccessToken: signBrokerJWT(t, key, access), TokenType: "Bearer", ExpiresIn: 600}
	}
	if _, err := client.profileFromToken(context.Background(), provider, discovery, makeToken(idBase, accessBase), "sent-nonce"); err != nil {
		t.Fatalf("valid Broker tokens rejected: %v", err)
	}
	tests := []struct {
		name   string
		id     bool
		mutate func(map[string]any)
	}{
		{"ID issuer trailing slash", true, func(c map[string]any) { c["iss"] = server.URL + "/" }},
		{"ID missing iat", true, func(c map[string]any) { delete(c, "iat") }},
		{"ID extra audience", true, func(c map[string]any) { c["aud"] = []string{"graphit-cli", "other-client"} }},
		{"ID wrong nonce", true, func(c map[string]any) { c["nonce"] = "other" }},
		{"access wrong audience", false, func(c map[string]any) { c["aud"] = "other-api" }},
		{"access wrong client", false, func(c map[string]any) { c["client_id"] = "other-client" }},
		{"access wrong subject", false, func(c map[string]any) { c["sub"] = "another-person" }},
		{"access wrong use", false, func(c map[string]any) { c["token_use"] = "id" }},
		{"access missing iat", false, func(c map[string]any) { delete(c, "iat") }},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			id, access := cloneClaims(idBase), cloneClaims(accessBase)
			if test.id {
				test.mutate(id)
			} else {
				test.mutate(access)
			}
			if _, err := client.profileFromToken(context.Background(), provider, discovery, makeToken(id, access), "sent-nonce"); err == nil {
				t.Fatal("invalid Broker token was accepted")
			}
		})
	}
	tampered := makeToken(idBase, accessBase)
	tampered.AccessToken = tamperBrokerJWT(tampered.AccessToken)
	if _, err := client.profileFromToken(context.Background(), provider, discovery, tampered, "sent-nonce"); err == nil {
		t.Fatal("Broker access token with invalid signature was accepted")
	}
	badType := makeToken(idBase, accessBase)
	badType.TokenType = "MAC"
	if _, err := client.profileFromToken(context.Background(), provider, discovery, badType, "sent-nonce"); err == nil {
		t.Fatal("non-Bearer Broker token was accepted")
	}
}

func TestBrokerCodeAndRefreshBindConfiguredMCPResource(t *testing.T) {
	_, key, err := ed25519.GenerateKey(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	now := time.Now().UTC()
	resource := "https://graphit.example.com/mcp"
	var requestedNonce string
	var server *httptest.Server
	requests := 0
	server = httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/.well-known/openid-configuration":
			writeJSON(t, w, oidcDiscoveryForBrokerTest(server.URL))
		case "/jwks":
			writeJSON(t, w, brokerJWKSForTest(key))
		case "/token":
			if err := r.ParseForm(); err != nil || r.Form.Get("resource") != resource {
				t.Errorf("token request omitted the configured MCP resource: %v %v", r.Form, err)
				http.Error(w, "invalid_target", http.StatusBadRequest)
				return
			}
			requests++
			access := brokerAccessClaimsForTest(server.URL, now, fmt.Sprintf("access-%d", requests))
			access["aud"] = []string{"graphit-broker", resource}
			if r.Form.Get("grant_type") == "authorization_code" {
				id := map[string]any{"iss": server.URL, "sub": "gb_sub_1", "aud": "graphit-cli", "iat": now.Unix(), "exp": now.Add(time.Hour).Unix(), "nonce": requestedNonce, "preferred_username": "alice"}
				writeStandardBrokerToken(t, w, key, id, signBrokerJWT(t, key, access), "refresh-1")
			} else {
				writeJSON(t, w, map[string]any{"access_token": signBrokerJWT(t, key, access), "refresh_token": "refresh-2", "token_type": "Bearer", "expires_in": 600})
			}
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
	provider := Provider{Name: "broker", Type: ProviderBroker, OIDC: &OIDCConfig{Issuer: server.URL, ClientID: "graphit-cli", MCPAudience: "graphit-broker", MCPResource: resource, UsernameClaim: "preferred_username"}}
	authorization, err := client.AuthorizationRequest(provider, discovery, "http://127.0.0.1:49152/callback")
	if err != nil {
		t.Fatal(err)
	}
	parsed, _ := url.Parse(authorization.URL)
	if parsed.Query().Get("resource") != resource || parsed.Query().Get("code_challenge_method") != "S256" {
		t.Fatalf("authorization request lacks resource or PKCE: %s", authorization.URL)
	}
	requestedNonce = authorization.Nonce
	profile, err := client.ExchangeCode(context.Background(), provider, discovery, "code", authorization.Verifier, "http://127.0.0.1:49152/callback", authorization.Nonce)
	if err != nil {
		t.Fatal(err)
	}
	refreshed, err := client.Refresh(context.Background(), provider, profile)
	if err != nil || refreshed.Subject != profile.Subject || refreshed.OIDC.RefreshToken != "refresh-2" || requests != 2 {
		t.Fatalf("refresh failed or missed resource: profile=%#v requests=%d err=%v", refreshed, requests, err)
	}
}

func TestBrokerOIDCRejectsCrossOriginEndpoint(t *testing.T) {
	var broker *httptest.Server
	broker = httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/.well-known/graphit-broker":
			writeJSON(t, w, brokerDiscoveryForTest(broker.URL))
		case "/.well-known/openid-configuration":
			document := oidcDiscoveryForBrokerTest(broker.URL)
			document["token_endpoint"] = "https://attacker.example/token"
			writeJSON(t, w, document)
		default:
			http.NotFound(w, r)
		}
	}))
	defer broker.Close()
	if _, err := BrokerOIDCProvider(context.Background(), brokerProviderForTest(broker.URL), broker.Client()); err == nil || !strings.Contains(err.Error(), "outside") {
		t.Fatalf("cross-origin endpoint accepted: %v", err)
	}
}

func TestBrokerOIDCRejectsCallbackStateMismatch(t *testing.T) {
	var broker *httptest.Server
	broker = httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/.well-known/graphit-broker":
			writeJSON(t, w, brokerDiscoveryForTest(broker.URL))
		case "/.well-known/openid-configuration":
			writeJSON(t, w, oidcDiscoveryForBrokerTest(broker.URL))
		default:
			http.NotFound(w, r)
		}
	}))
	defer broker.Close()
	resolved, err := BrokerOIDCProvider(context.Background(), brokerProviderForTest(broker.URL), broker.Client())
	if err != nil {
		t.Fatal(err)
	}
	_, err = (&OIDCClient{HTTP: broker.Client()}).LoginInteractive(context.Background(), resolved, func(target string) error {
		parsed, parseErr := url.Parse(target)
		if parseErr != nil {
			return parseErr
		}
		go func() {
			response, requestErr := http.Get(parsed.Query().Get("redirect_uri") + "?code=code&state=attacker")
			if requestErr == nil {
				_ = response.Body.Close()
			}
		}()
		return nil
	})
	if err == nil || !strings.Contains(err.Error(), "state") {
		t.Fatalf("state mismatch accepted: %v", err)
	}
}

func brokerProviderForTest(endpoint string) Provider {
	return Provider{Name: "company", Type: ProviderBroker, Revision: 1, Broker: &BrokerConfig{Endpoint: endpoint},
		AI: AIConfig{Embedding: AIServiceConfig{Mode: ServiceBroker}, Rerank: AIServiceConfig{Mode: ServiceBroker}}}
}

func brokerDiscoveryForTest(issuer string) map[string]any {
	return map[string]any{"version": "1", "issuer": issuer, "authentication": map[string]any{
		"type": "openid_connect", "issuer": issuer, "client_id": "graphit-cli",
		"scopes": []string{"openid", "profile", "email", "offline_access"}, "redirect_uri_path": "/oauth/callback",
		"audiences": []string{"graphit-broker"}, "access_token_audience": "graphit-broker",
		"mcp_resources": []string{"https://graphit.example.com/mcp"}}}
}

func oidcDiscoveryForBrokerTest(issuer string) map[string]any {
	return map[string]any{"issuer": issuer, "authorization_endpoint": issuer + "/authorize", "token_endpoint": issuer + "/token",
		"jwks_uri": issuer + "/jwks", "userinfo_endpoint": issuer + "/userinfo",
		"grant_types_supported": []string{"authorization_code", "refresh_token"}, "code_challenge_methods_supported": []string{"S256"},
		"token_endpoint_auth_methods_supported": []string{"none"}, "id_token_signing_alg_values_supported": []string{"EdDSA"}}
}

func brokerAccessClaimsForTest(issuer string, now time.Time, jwtID string) map[string]any {
	return map[string]any{
		"iss": issuer, "sub": "gb_sub_1", "aud": []string{"graphit-broker", "https://graphit.example.com/mcp"}, "exp": now.Add(time.Hour).Unix(), "iat": now.Unix(),
		"jti": jwtID, "client_id": "graphit-cli", "token_use": "access", "scope": "openid profile email offline_access",
		"preferred_username": "alice", "organization": "acme", "groups": []string{"platform"},
	}
}

func tamperBrokerJWT(raw string) string {
	parts := strings.Split(raw, ".")
	if len(parts) != 3 || parts[2] == "" {
		return raw + "invalid"
	}
	replacement := byte('A')
	if parts[2][0] == replacement {
		replacement = 'B'
	}
	parts[2] = string(replacement) + parts[2][1:]
	return strings.Join(parts, ".")
}

func brokerJWKSForTest(key ed25519.PrivateKey) map[string]any {
	public := key.Public().(ed25519.PublicKey)
	return map[string]any{"keys": []any{map[string]any{"kty": "OKP", "kid": "test", "alg": "EdDSA", "crv": "Ed25519",
		"x": base64.RawURLEncoding.EncodeToString(public)}}}
}

func signBrokerJWT(t *testing.T, key ed25519.PrivateKey, claims map[string]any) string {
	t.Helper()
	header, err := json.Marshal(map[string]any{"alg": "EdDSA", "kid": "test", "typ": "JWT"})
	if err != nil {
		t.Fatal(err)
	}
	payload, err := json.Marshal(claims)
	if err != nil {
		t.Fatal(err)
	}
	content := base64.RawURLEncoding.EncodeToString(header) + "." + base64.RawURLEncoding.EncodeToString(payload)
	return content + "." + base64.RawURLEncoding.EncodeToString(ed25519.Sign(key, []byte(content)))
}

func writeStandardBrokerToken(t *testing.T, w http.ResponseWriter, key ed25519.PrivateKey, claims map[string]any, access, refresh string) {
	writeJSON(t, w, map[string]any{"access_token": access, "refresh_token": refresh, "id_token": signBrokerJWT(t, key, claims), "token_type": "Bearer", "expires_in": 600})
}

func sha256Sum(value string) string {
	digest := sha256.Sum256([]byte(value))
	return base64.RawURLEncoding.EncodeToString(digest[:])
}
