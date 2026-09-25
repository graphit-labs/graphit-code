package uiserver

import (
	"crypto/ed25519"
	"crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"net/url"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/auth"
)

func signedWebTestJWT(t *testing.T, key ed25519.PrivateKey, claims map[string]any) string {
	t.Helper()
	header, _ := json.Marshal(map[string]string{"alg": "EdDSA", "kid": "key-1", "typ": "JWT"})
	payload, _ := json.Marshal(claims)
	input := base64.RawURLEncoding.EncodeToString(header) + "." + base64.RawURLEncoding.EncodeToString(payload)
	return input + "." + base64.RawURLEncoding.EncodeToString(ed25519.Sign(key, []byte(input)))
}

func TestWebBrokerLoginRoundTripUsesCookieWithoutGlobalProfile(t *testing.T) {
	public, private, err := ed25519.GenerateKey(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	var broker *httptest.Server
	var loginNonce, challenge string
	var loginMu sync.Mutex
	var revoked atomic.Bool
	var refreshCalls atomic.Int32
	broker = httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		switch r.URL.Path {
		case "/.well-known/graphit-broker":
			_ = json.NewEncoder(w).Encode(map[string]any{"version": "1", "issuer": broker.URL, "authentication": map[string]any{"type": "openid_connect", "issuer": broker.URL, "client_id": "graphit-cli", "scopes": []string{"openid", "profile", "email", "offline_access"}, "redirect_uri_path": "/oauth/callback", "audiences": []string{"graphit-broker"}, "access_token_audience": "graphit-broker"}})
		case "/.well-known/openid-configuration":
			_ = json.NewEncoder(w).Encode(map[string]any{"issuer": broker.URL, "authorization_endpoint": broker.URL + "/authorize", "token_endpoint": broker.URL + "/token", "registration_endpoint": broker.URL + "/oauth/register", "revocation_endpoint": broker.URL + "/oauth/revoke", "jwks_uri": broker.URL + "/jwks", "userinfo_endpoint": broker.URL + "/userinfo", "grant_types_supported": []string{"authorization_code", "refresh_token"}, "code_challenge_methods_supported": []string{"S256"}, "token_endpoint_auth_methods_supported": []string{"none"}, "id_token_signing_alg_values_supported": []string{"EdDSA"}})
		case "/oauth/register":
			w.WriteHeader(http.StatusCreated)
			_ = json.NewEncoder(w).Encode(map[string]string{"client_id": "web-client"})
		case "/jwks":
			_ = json.NewEncoder(w).Encode(map[string]any{"keys": []any{map[string]string{"kty": "OKP", "crv": "Ed25519", "kid": "key-1", "alg": "EdDSA", "x": base64.RawURLEncoding.EncodeToString(public)}}})
		case "/token":
			if err := r.ParseForm(); err != nil {
				t.Error(err)
			}
			if r.Form.Get("client_id") != "web-client" || r.Form.Get("resource") != broker.URL+"/v1" {
				t.Errorf("unexpected token request: %v", r.Form)
			}
			now := time.Now().Unix()
			if r.Form.Get("grant_type") == "refresh_token" {
				refreshCalls.Add(1)
				if r.Form.Get("refresh_token") != "refresh-1" {
					t.Errorf("wrong refresh grant: %v", r.Form)
				}
				access := signedWebTestJWT(t, private, map[string]any{"iss": broker.URL, "sub": "person-1", "aud": []string{"graphit-broker", broker.URL + "/v1"}, "iat": now, "exp": now + 3600, "jti": "access-2", "client_id": "web-client", "token_use": "access", "preferred_username": "Alice"})
				_ = json.NewEncoder(w).Encode(map[string]any{"access_token": access, "refresh_token": "refresh-2", "token_type": "Bearer", "expires_in": 3600})
				return
			}
			if r.Form.Get("redirect_uri") != "http://127.0.0.1:8080"+webCallbackPath || r.Form.Get("code") != "valid-code" {
				t.Errorf("unexpected code grant: %v", r.Form)
			}
			digest := sha256.Sum256([]byte(r.Form.Get("code_verifier")))
			loginMu.Lock()
			wantChallenge, nonce := challenge, loginNonce
			loginMu.Unlock()
			if base64.RawURLEncoding.EncodeToString(digest[:]) != wantChallenge {
				t.Error("PKCE verifier did not match challenge")
			}
			idToken := signedWebTestJWT(t, private, map[string]any{"iss": broker.URL, "sub": "person-1", "aud": "web-client", "iat": now, "exp": now + 3600, "nonce": nonce, "preferred_username": "Alice"})
			access := signedWebTestJWT(t, private, map[string]any{"iss": broker.URL, "sub": "person-1", "aud": []string{"graphit-broker", broker.URL + "/v1"}, "iat": now, "exp": now + 3600, "jti": "access-1", "client_id": "web-client", "token_use": "access", "preferred_username": "Alice"})
			_ = json.NewEncoder(w).Encode(map[string]any{"access_token": access, "refresh_token": "refresh-1", "id_token": idToken, "token_type": "Bearer", "expires_in": 60})
		case "/oauth/revoke":
			if err := r.ParseForm(); err != nil {
				t.Error(err)
			}
			if r.Form.Get("token") != "refresh-2" || r.Form.Get("client_id") != "web-client" {
				t.Errorf("wrong token revocation: %v", r.Form)
			}
			revoked.Store(true)
			w.WriteHeader(http.StatusOK)
		default:
			http.NotFound(w, r)
		}
	}))
	defer broker.Close()
	dir := t.TempDir()
	t.Setenv("GRAPHIT_GLOBAL_DIR", dir)
	state := auth.State{Version: auth.StateVersion, Providers: map[string]auth.Provider{"company": {Name: "company", Type: auth.ProviderBroker, Revision: 1, Broker: &auth.BrokerConfig{Endpoint: broker.URL}}}, Profiles: map[string]auth.Profile{}}
	data, _ := json.Marshal(state)
	if err := os.WriteFile(filepath.Join(dir, "auth.json"), data, 0o600); err != nil {
		t.Fatal(err)
	}
	a, err := newWebAuth(true, false, "http://127.0.0.1:8080")
	if err != nil {
		t.Fatal(err)
	}
	a.client.HTTP = broker.Client()
	login := httptest.NewRecorder()
	a.login(login, httptest.NewRequest(http.MethodPost, "http://127.0.0.1:8080/api/auth/login", strings.NewReader(`{"provider":"company"}`)))
	if login.Code != http.StatusOK {
		t.Fatalf("login start status=%d body=%s", login.Code, login.Body.String())
	}
	var start struct {
		AuthorizationURL string `json:"authorization_url"`
	}
	if err := json.Unmarshal(login.Body.Bytes(), &start); err != nil {
		t.Fatal(err)
	}
	authorization, err := url.Parse(start.AuthorizationURL)
	if err != nil {
		t.Fatal(err)
	}
	loginMu.Lock()
	loginNonce, challenge = authorization.Query().Get("nonce"), authorization.Query().Get("code_challenge")
	loginMu.Unlock()
	if loginNonce == "" || challenge == "" || authorization.Query().Get("resource") != broker.URL+"/v1" {
		t.Fatalf("OIDC request=%s", start.AuthorizationURL)
	}
	callback := httptest.NewRequest(http.MethodGet, "http://127.0.0.1:8080"+webCallbackPath+"?code=valid-code&state="+url.QueryEscape(authorization.Query().Get("state")), nil)
	callback.AddCookie(login.Result().Cookies()[0])
	finished := httptest.NewRecorder()
	a.callback(finished, callback)
	if finished.Code != http.StatusSeeOther {
		t.Fatalf("callback status=%d body=%s", finished.Code, finished.Body.String())
	}
	var sessionCookie *http.Cookie
	for _, cookie := range finished.Result().Cookies() {
		if cookie.Name == webSessionCookie {
			sessionCookie = cookie
		}
	}
	if sessionCookie == nil || !sessionCookie.HttpOnly {
		t.Fatal("verified browser session cookie missing")
	}
	loaded, err := auth.Open()
	if err != nil {
		t.Fatal(err)
	}
	unchanged, err := loaded.Load()
	if err != nil || len(unchanged.Profiles) != 0 {
		t.Fatalf("web login persisted global profile: %v, %v", unchanged.Profiles, err)
	}
	mux := http.NewServeMux()
	mux.HandleFunc("/api/data", func(w http.ResponseWriter, r *http.Request) {
		snapshot, err := auth.ResolveActive(r.Context())
		if err != nil || snapshot.Profile.Username != "Alice" {
			t.Errorf("browser identity=%+v err=%v", snapshot.Profile, err)
		}
		w.WriteHeader(http.StatusNoContent)
	})
	req := httptest.NewRequest(http.MethodGet, "http://127.0.0.1:8080/api/data", nil)
	req.AddCookie(sessionCookie)
	w := httptest.NewRecorder()
	a.wrap(mux).ServeHTTP(w, req)
	if w.Code != http.StatusNoContent {
		t.Fatalf("browser API status=%d", w.Code)
	}
	// Two requests with the same stale cookie must share one rotated refresh grant.
	var group sync.WaitGroup
	for i := 0; i < 2; i++ {
		group.Add(1)
		go func() {
			defer group.Done()
			replayed := httptest.NewRequest(http.MethodGet, "http://127.0.0.1:8080/api/data", nil)
			replayed.AddCookie(sessionCookie)
			result := httptest.NewRecorder()
			a.wrap(mux).ServeHTTP(result, replayed)
			if result.Code != http.StatusNoContent {
				t.Errorf("concurrent cookie status=%d", result.Code)
			}
		}()
	}
	group.Wait()
	if refreshCalls.Load() != 1 {
		t.Fatalf("rotating refresh was called %d times", refreshCalls.Load())
	}
	logout := httptest.NewRequest(http.MethodPost, "http://127.0.0.1:8080/api/auth/logout", nil)
	logout.Header.Set("X-Graphit-Request", "ui")
	logout.AddCookie(sessionCookie)
	out := httptest.NewRecorder()
	a.wrap(http.HandlerFunc(a.logout)).ServeHTTP(out, logout)
	if out.Code != http.StatusOK || !revoked.Load() {
		t.Fatalf("logout status=%d revoked=%t body=%s", out.Code, revoked.Load(), out.Body.String())
	}
	w = httptest.NewRecorder()
	a.wrap(mux).ServeHTTP(w, req)
	if w.Code != http.StatusUnauthorized {
		t.Fatalf("revoked cookie status=%d", w.Code)
	}
}

func TestWebAuthCookieIsEncryptedHttpOnlyAndSecureByDefault(t *testing.T) {
	a, err := newWebAuth(true, true, "https://code.example")
	if err != nil {
		t.Fatal(err)
	}
	profile := auth.Profile{Username: "Alice", OIDC: &auth.OIDCSession{AccessToken: "secret-access", RefreshToken: "secret-refresh", IDToken: "secret-id", ExpiresAt: time.Now().Add(time.Hour)}}
	w := httptest.NewRecorder()
	if err := a.setSession(w, webSession{ID: "session-1", Expires: time.Now().Add(7 * 24 * time.Hour).Unix(), Provider: "company", Revision: 1, ClientID: "client", Profile: profile}); err != nil {
		t.Fatal(err)
	}
	cookies := w.Result().Cookies()
	if len(cookies) != 1 || !cookies[0].HttpOnly || !cookies[0].Secure || cookies[0].SameSite != http.SameSiteStrictMode {
		t.Fatalf("cookie flags: %+v", cookies)
	}
	if strings.Contains(cookies[0].Value, "secret") {
		t.Fatal("authentication material is visible in cookie ciphertext")
	}
	var decoded webSession
	if err := a.open(cookies[0].Value, &decoded); err != nil {
		t.Fatal(err)
	}
	if decoded.Profile.OIDC.AccessToken != "secret-access" || decoded.Profile.OIDC.IDToken != "" {
		t.Fatalf("cookie session=%+v", decoded.Profile.OIDC)
	}
	other, _ := newWebAuth(true, true, "https://code.example")
	if err := other.open(cookies[0].Value, &decoded); err == nil {
		t.Fatal("another server key accepted the cookie")
	}
	local, err := newWebAuth(true, false, "http://127.0.0.1:8080")
	if err != nil {
		t.Fatal(err)
	}
	if local.cookie("test", "value", 100, http.SameSiteStrictMode).Secure {
		t.Fatal("Secure flag could not be disabled")
	}
}

func TestWebAuthUsesBrowserIdentityAndRejectsAnonymousAPIs(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("GRAPHIT_GLOBAL_DIR", dir)
	state := auth.State{Version: auth.StateVersion, Providers: map[string]auth.Provider{"company": {
		Name: "company", Type: auth.ProviderBroker, Revision: 1, Broker: &auth.BrokerConfig{Endpoint: "https://broker.example"},
	}}, Profiles: map[string]auth.Profile{}}
	data, err := json.Marshal(state)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "auth.json"), data, 0o600); err != nil {
		t.Fatal(err)
	}
	a, err := newWebAuth(true, true, "https://code.example")
	if err != nil {
		t.Fatal(err)
	}
	mux := http.NewServeMux()
	mux.HandleFunc("/api/auth/session", a.session)
	mux.HandleFunc("/api/data", func(w http.ResponseWriter, r *http.Request) {
		snapshot, err := auth.ResolveActive(r.Context())
		if err != nil {
			t.Errorf("request profile: %v", err)
			return
		}
		if snapshot.Profile.Username != "Alice" || auth.RequestBrokerBearer(r.Context()) != "browser-token" {
			t.Errorf("wrong browser identity: %+v", snapshot.Profile)
		}
		w.WriteHeader(http.StatusNoContent)
	})
	unauthenticated := httptest.NewRecorder()
	a.wrap(mux).ServeHTTP(unauthenticated, httptest.NewRequest(http.MethodGet, "https://code.example/api/data", nil))
	if unauthenticated.Code != http.StatusUnauthorized {
		t.Fatalf("anonymous API status=%d", unauthenticated.Code)
	}
	anonymous := httptest.NewRecorder()
	a.wrap(mux).ServeHTTP(anonymous, httptest.NewRequest(http.MethodGet, "https://code.example/api/auth/session", nil))
	if anonymous.Code != http.StatusOK || !strings.Contains(anonymous.Body.String(), `"username":"Anônimo"`) {
		t.Fatalf("anonymous session=%s", anonymous.Body.String())
	}
	profile := auth.Profile{Username: "Alice", OIDC: &auth.OIDCSession{AccessToken: "browser-token", RefreshToken: "refresh", ExpiresAt: time.Now().Add(time.Hour)}}
	cookieResponse := httptest.NewRecorder()
	if err := a.setSession(cookieResponse, webSession{ID: "session-1", Expires: time.Now().Add(7 * 24 * time.Hour).Unix(), Provider: "company", Revision: 1, ClientID: "web-client", Profile: profile}); err != nil {
		t.Fatal(err)
	}
	req := httptest.NewRequest(http.MethodGet, "https://code.example/api/data", nil)
	req.AddCookie(cookieResponse.Result().Cookies()[0])
	response := httptest.NewRecorder()
	a.wrap(mux).ServeHTTP(response, req)
	if response.Code != http.StatusNoContent {
		t.Fatalf("authenticated API status=%d body=%s", response.Code, response.Body.String())
	}
	mutating := httptest.NewRequest(http.MethodPost, "https://code.example/api/data", nil)
	mutating.AddCookie(cookieResponse.Result().Cookies()[0])
	csrf := httptest.NewRecorder()
	a.wrap(mux).ServeHTTP(csrf, mutating)
	if csrf.Code != http.StatusForbidden {
		t.Fatalf("CSRF status=%d", csrf.Code)
	}
	crossOrigin := httptest.NewRequest(http.MethodPost, "https://code.example/api/data", nil)
	crossOrigin.Header.Set("Origin", "https://evil.example")
	crossOrigin.Header.Set("X-Graphit-Request", "ui")
	crossOrigin.AddCookie(cookieResponse.Result().Cookies()[0])
	denied := httptest.NewRecorder()
	a.wrap(mux).ServeHTTP(denied, crossOrigin)
	if denied.Code != http.StatusForbidden {
		t.Fatalf("foreign origin status=%d", denied.Code)
	}
	expiredCookie, err := a.seal(webSession{ID: "expired", Expires: time.Now().Add(-time.Minute).Unix(), Provider: "company", Revision: 1, ClientID: "web-client", Profile: profile})
	if err != nil {
		t.Fatal(err)
	}
	expired := httptest.NewRequest(http.MethodGet, "https://code.example/api/data", nil)
	expired.AddCookie(a.cookie(webSessionCookie, expiredCookie, 7*24*3600, http.SameSiteStrictMode))
	expiredResponse := httptest.NewRecorder()
	a.wrap(mux).ServeHTTP(expiredResponse, expired)
	if expiredResponse.Code != http.StatusUnauthorized {
		t.Fatalf("expired cookie status=%d", expiredResponse.Code)
	}
}

func TestWebAuthCallbackRejectsMismatchedState(t *testing.T) {
	a, err := newWebAuth(true, false, "http://127.0.0.1:8080")
	if err != nil {
		t.Fatal(err)
	}
	value, err := a.seal(webLogin{State: "expected", Issued: time.Now().Unix(), Redirect: "http://127.0.0.1:8080" + webCallbackPath})
	if err != nil {
		t.Fatal(err)
	}
	req := httptest.NewRequest(http.MethodGet, "http://127.0.0.1:8080"+webCallbackPath+"?state=wrong&code=code", nil)
	req.AddCookie(a.cookie(webLoginCookie, value, 600, http.SameSiteLaxMode))
	w := httptest.NewRecorder()
	a.callback(w, req)
	if w.Code != http.StatusBadRequest {
		t.Fatalf("callback status=%d", w.Code)
	}
	if len(w.Result().Cookies()) != 1 || w.Result().Cookies()[0].MaxAge != -1 {
		t.Fatal("login cookie was not cleared")
	}
}
