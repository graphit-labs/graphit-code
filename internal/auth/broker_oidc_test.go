package auth

import (
	"context"
	"crypto/rand"
	"crypto/rsa"
	"crypto/sha256"
	"encoding/base64"
	"math/big"
	"net/http"
	"net/http/httptest"
	"net/url"
	"strings"
	"testing"
	"time"
)

func TestBrokerLoginAndAccessTokenVerificationUseStandardOIDCAndJWKS(t *testing.T) {
	key, err := rsa.GenerateKey(rand.Reader, 2048)
	if err != nil {
		t.Fatal(err)
	}
	fixed := time.Date(2026, 9, 9, 12, 0, 0, 0, time.UTC)
	var server *httptest.Server
	var nonce, challenge string
	userinfoCalls := 0
	server = httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/.well-known/graphit-broker":
			writeJSON(t, w, brokerDiscoveryForTest(server.URL))
		case "/.well-known/openid-configuration":
			writeJSON(t, w, oidcDiscoveryForBrokerTest(server.URL))
		case "/jwks":
			writeJSON(t, w, rsaJWKSForBrokerTest(key))
		case "/token":
			if err := r.ParseForm(); err != nil {
				t.Error(err)
			}
			claims := map[string]any{"iss": server.URL, "sub": "gb_sub_1", "aud": "graphit-cli", "exp": fixed.Add(time.Hour).Unix(),
				"preferred_username": "alice", "organization": "acme", "groups": []string{"platform"}}
			switch r.Form.Get("grant_type") {
			case "authorization_code":
				digest := sha256Sum(r.Form.Get("code_verifier"))
				if r.Form.Get("code") != "broker-code" || digest != challenge || r.Form.Get("redirect_uri") == "" {
					t.Fatalf("invalid code exchange: %#v", r.Form)
				}
				claims["nonce"] = nonce
				writeStandardBrokerToken(t, w, key, claims, signJWT(t, key, brokerAccessClaimsForTest(server.URL, fixed, "access-1")), "refresh-1")
			case "refresh_token":
				if r.Form.Get("refresh_token") != "refresh-1" {
					t.Fatalf("refresh token=%q", r.Form.Get("refresh_token"))
				}
				writeStandardBrokerToken(t, w, key, claims, signJWT(t, key, brokerAccessClaimsForTest(server.URL, fixed, "access-2")), "refresh-2")
			default:
				http.Error(w, "unexpected grant", http.StatusBadRequest)
			}
		case "/userinfo":
			userinfoCalls++
			http.Error(w, "userinfo must not be used for access-token verification", http.StatusInternalServerError)
		default:
			http.NotFound(w, r)
		}
	}))
	defer server.Close()

	provider := brokerProviderForTest(server.URL)
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
	refreshed, err := client.Refresh(context.Background(), resolved, profile)
	if err != nil || refreshed.OIDC.AccessToken == "" || refreshed.OIDC.AccessToken == profile.OIDC.AccessToken || refreshed.OIDC.RefreshToken != "refresh-2" || refreshed.Subject != profile.Subject {
		t.Fatalf("refreshed=%#v err=%v", refreshed, err)
	}
	verifier := &ProviderAccessTokenVerifier{OIDC: client}
	identity, err := verifier.VerifyAccessToken(context.Background(), provider, profile.OIDC.AccessToken, "")
	if err != nil || identity.Issuer != server.URL || identity.Username != "alice" || identity.Subject != "gb_sub_1" {
		t.Fatalf("verified broker identity=%#v err=%v", identity, err)
	}
	invalid := []struct {
		name   string
		mutate func(map[string]any)
	}{
		{name: "issuer", mutate: func(claims map[string]any) { claims["iss"] = "https://other.example" }},
		{name: "audience", mutate: func(claims map[string]any) { claims["aud"] = "other-api" }},
		{name: "expired", mutate: func(claims map[string]any) { claims["exp"] = fixed.Add(-time.Minute).Unix() }},
		{name: "client", mutate: func(claims map[string]any) { claims["client_id"] = "other-client" }},
		{name: "scope", mutate: func(claims map[string]any) { claims["scope"] = "openid profile" }},
		{name: "username", mutate: func(claims map[string]any) { delete(claims, "preferred_username") }},
	}
	for _, test := range invalid {
		t.Run("reject "+test.name, func(t *testing.T) {
			claims := brokerAccessClaimsForTest(server.URL, fixed, "invalid-"+test.name)
			test.mutate(claims)
			if _, err := verifier.VerifyAccessToken(context.Background(), provider, signJWT(t, key, claims), "ignored-by-broker"); err == nil {
				t.Fatal("invalid Broker access token was accepted")
			}
		})
	}
	if _, err := verifier.VerifyAccessToken(context.Background(), provider, tamperBrokerJWT(profile.OIDC.AccessToken), ""); err == nil {
		t.Fatal("Broker access token with tampered signature was accepted")
	}
	if userinfoCalls != 0 {
		t.Fatalf("Broker access-token verification called userinfo %d times", userinfoCalls)
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
	return Provider{Name: "company", Type: ProviderBroker, Revision: 1, Broker: &BrokerConfig{Endpoint: endpoint, TokenStrategy: "relay"},
		AI: AIConfig{Embedding: AIServiceConfig{Mode: ServiceBroker}, Rerank: AIServiceConfig{Mode: ServiceBroker}}}
}

func brokerDiscoveryForTest(issuer string) map[string]any {
	return map[string]any{"version": "1", "issuer": issuer, "authentication": map[string]any{
		"type": "openid_connect", "issuer": issuer, "client_id": "graphit-cli",
		"scopes": []string{"openid", "profile", "email", "graphit.use", "offline_access"}, "redirect_uri_path": "/oauth/callback",
		"audiences": []string{"graphit-broker"}, "access_token_audience": "graphit-broker"}}
}

func oidcDiscoveryForBrokerTest(issuer string) map[string]any {
	return map[string]any{"issuer": issuer, "authorization_endpoint": issuer + "/authorize", "token_endpoint": issuer + "/token",
		"jwks_uri":              issuer + "/jwks",
		"grant_types_supported": []string{"authorization_code", "refresh_token"}, "code_challenge_methods_supported": []string{"S256"},
		"token_endpoint_auth_methods_supported": []string{"none"}, "id_token_signing_alg_values_supported": []string{"EdDSA"}}
}

func brokerAccessClaimsForTest(issuer string, now time.Time, jwtID string) map[string]any {
	return map[string]any{
		"iss": issuer, "sub": "gb_sub_1", "aud": "graphit-broker", "exp": now.Add(time.Hour).Unix(), "iat": now.Unix(),
		"jti": jwtID, "client_id": "graphit-cli", "scope": "openid profile email graphit.use offline_access",
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

func rsaJWKSForBrokerTest(key *rsa.PrivateKey) map[string]any {
	return map[string]any{"keys": []any{map[string]any{"kty": "RSA", "kid": "test", "alg": "RS256",
		"n": base64.RawURLEncoding.EncodeToString(key.N.Bytes()), "e": base64.RawURLEncoding.EncodeToString(big.NewInt(int64(key.E)).Bytes())}}}
}

func writeStandardBrokerToken(t *testing.T, w http.ResponseWriter, key *rsa.PrivateKey, claims map[string]any, access, refresh string) {
	writeJSON(t, w, map[string]any{"access_token": access, "refresh_token": refresh, "id_token": signJWT(t, key, claims), "token_type": "Bearer", "expires_in": 600})
}

func sha256Sum(value string) string {
	digest := sha256.Sum256([]byte(value))
	return base64.RawURLEncoding.EncodeToString(digest[:])
}
