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

func TestBrokerLoginUsesStandardOIDCAndUserinfo(t *testing.T) {
	key, err := rsa.GenerateKey(rand.Reader, 2048)
	if err != nil {
		t.Fatal(err)
	}
	fixed := time.Date(2026, 9, 9, 12, 0, 0, 0, time.UTC)
	var server *httptest.Server
	var nonce, challenge string
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
				writeStandardBrokerToken(t, w, key, claims, "access-1", "refresh-1")
			case "refresh_token":
				if r.Form.Get("refresh_token") != "refresh-1" {
					t.Fatalf("refresh token=%q", r.Form.Get("refresh_token"))
				}
				writeStandardBrokerToken(t, w, key, claims, "access-2", "refresh-2")
			default:
				http.Error(w, "unexpected grant", http.StatusBadRequest)
			}
		case "/userinfo":
			if r.Header.Get("Authorization") != "Bearer inbound-token" {
				http.Error(w, "unauthorized", http.StatusUnauthorized)
				return
			}
			writeJSON(t, w, map[string]any{"sub": "gb_sub_1", "preferred_username": "alice", "organization": "acme", "groups": []string{"platform"}})
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
	if !strings.HasPrefix(openedURL, server.URL+"/authorize?") || profile.Username != "alice" || profile.Subject != "gb_sub_1" || profile.Issuer != server.URL || profile.OIDC == nil || profile.OIDC.AccessToken != "access-1" || profile.OIDC.RefreshToken != "refresh-1" {
		t.Fatalf("opened=%q profile=%#v", openedURL, profile)
	}
	refreshed, err := client.Refresh(context.Background(), resolved, profile)
	if err != nil || refreshed.OIDC.AccessToken != "access-2" || refreshed.OIDC.RefreshToken != "refresh-2" || refreshed.Subject != profile.Subject {
		t.Fatalf("refreshed=%#v err=%v", refreshed, err)
	}
	identity, err := verifyBrokerAccessToken(context.Background(), client, provider, "inbound-token")
	if err != nil || identity.Issuer != server.URL || identity.Username != "alice" || identity.Subject != "gb_sub_1" {
		t.Fatalf("verified broker identity=%#v err=%v", identity, err)
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
		"scopes": []string{"openid", "profile", "email", "graphit.use", "offline_access"}, "redirect_uri_path": "/oauth/callback"}}
}

func oidcDiscoveryForBrokerTest(issuer string) map[string]any {
	return map[string]any{"issuer": issuer, "authorization_endpoint": issuer + "/authorize", "token_endpoint": issuer + "/token",
		"userinfo_endpoint": issuer + "/userinfo", "jwks_uri": issuer + "/jwks",
		"grant_types_supported": []string{"authorization_code", "refresh_token"}, "code_challenge_methods_supported": []string{"S256"},
		"token_endpoint_auth_methods_supported": []string{"none"}, "id_token_signing_alg_values_supported": []string{"EdDSA"}}
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
