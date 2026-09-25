package uiserver

import (
	"context"
	"crypto/aes"
	"crypto/cipher"
	"crypto/rand"
	"crypto/sha256"
	"crypto/subtle"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"sort"
	"strings"
	"sync"
	"time"

	"github.com/graphit-labs/graphit-code/internal/auth"
)

const webSessionCookie = "graphit_web_session"
const webLoginCookie = "graphit_web_login"
const webCallbackPath = "/api/auth/callback"

type webSession struct {
	ID       string       `json:"id"`
	Expires  int64        `json:"expires"`
	Provider string       `json:"provider"`
	Revision uint64       `json:"revision"`
	ClientID string       `json:"client_id"`
	Profile  auth.Profile `json:"profile"`
}

type webLogin struct {
	Provider string `json:"provider"`
	Revision uint64 `json:"revision"`
	ClientID string `json:"client_id"`
	Redirect string `json:"redirect"`
	State    string `json:"state"`
	Nonce    string `json:"nonce"`
	Verifier string `json:"verifier"`
	Issued   int64  `json:"issued"`
}

type webAuth struct {
	enabled   bool
	secure    bool
	publicURL string
	key       []byte
	client    *auth.OIDCClient
	mu        sync.Mutex
	clients   map[string]string
	used      map[string]time.Time
	refreshed map[[32]byte]webSession
}

func newWebAuth(enabled, secure bool, publicURL, encryptionKey string) (*webAuth, error) {
	a := &webAuth{enabled: enabled, secure: secure, publicURL: publicURL, client: auth.NewOIDCClient(), clients: map[string]string{}, used: map[string]time.Time{}, refreshed: map[[32]byte]webSession{}}
	if !enabled {
		return a, nil
	}
	if encryptionKey != "" {
		if len(encryptionKey) < 32 {
			return nil, errors.New("ui.auth.cookie_encryption_key must contain at least 32 bytes")
		}
		key := sha256.Sum256([]byte(encryptionKey))
		a.key = key[:]
	} else {
		a.key = make([]byte, 32)
		if _, err := rand.Read(a.key); err != nil {
			return nil, fmt.Errorf("generate web authentication key: %w", err)
		}
	}
	if publicURL != "" {
		u, err := url.Parse(publicURL)
		validScheme := err == nil && (u.Scheme == "https" || (u.Scheme == "http" && !secure))
		if err != nil || u.Host == "" || u.User != nil || u.RawQuery != "" || u.Fragment != "" || u.Path != "" || !validScheme {
			return nil, errors.New("ui.auth.public_url must be an origin (HTTPS, or HTTP with ui.auth.cookie_secure=false)")
		}
	}
	return a, nil
}

func (a *webAuth) origin(r *http.Request) (string, error) {
	if a.publicURL != "" {
		return a.publicURL, nil
	}
	// Do not use untrusted forwarded headers or arbitrary Host values to register
	// OAuth callbacks. External deployments configure their canonical public URL.
	host := r.URL.Hostname()
	if host == "" {
		host = r.Host
		if parsed, err := url.Parse("http://" + r.Host); err == nil {
			host = parsed.Hostname()
		}
	}
	if host != "localhost" && host != "127.0.0.1" && host != "::1" {
		return "", errors.New("set ui.auth.public_url to the external HTTPS origin")
	}
	if a.secure && r.TLS == nil {
		return "", errors.New("HTTPS is required while ui.auth.cookie_secure is true")
	}
	scheme := "http"
	if r.TLS != nil {
		scheme = "https"
	}
	return scheme + "://" + r.Host, nil
}

func (a *webAuth) seal(value any) (string, error) {
	plain, err := json.Marshal(value)
	if err != nil {
		return "", err
	}
	block, err := aes.NewCipher(a.key)
	if err != nil {
		return "", err
	}
	gcm, err := cipher.NewGCM(block)
	if err != nil {
		return "", err
	}
	nonce := make([]byte, gcm.NonceSize())
	if _, err := rand.Read(nonce); err != nil {
		return "", err
	}
	return "e." + base64.RawURLEncoding.EncodeToString(gcm.Seal(nonce, nonce, plain, nil)), nil
}

func (a *webAuth) open(raw string, dst any) error {
	if !strings.HasPrefix(raw, "e.") {
		return errors.New("invalid web cookie format")
	}
	data, err := base64.RawURLEncoding.DecodeString(strings.TrimPrefix(raw, "e."))
	if err != nil {
		return err
	}
	block, err := aes.NewCipher(a.key)
	if err != nil {
		return err
	}
	gcm, err := cipher.NewGCM(block)
	if err != nil || len(data) < gcm.NonceSize() {
		return errors.New("invalid web cookie")
	}
	plain, err := gcm.Open(nil, data[:gcm.NonceSize()], data[gcm.NonceSize():], nil)
	if err != nil {
		return err
	}
	return json.Unmarshal(plain, dst)
}

func (a *webAuth) cookie(name, value string, age int, sameSite http.SameSite) *http.Cookie {
	// #nosec G124 -- Secure is configurable for HTTP development; HttpOnly and SameSite stay enabled.
	return &http.Cookie{Name: name, Value: value, Path: "/", HttpOnly: true, Secure: a.secure, SameSite: sameSite, MaxAge: age}
}

func (a *webAuth) setSession(w http.ResponseWriter, session webSession) error {
	// The ID token has already been verified; retain only access and refresh tokens.
	if session.Profile.OIDC != nil {
		copyOIDC := *session.Profile.OIDC
		copyOIDC.IDToken = ""
		session.Profile.OIDC = &copyOIDC
	}
	value, err := a.seal(session)
	if err != nil {
		return err
	}
	if len(value) > 3800 {
		return errors.New("broker session exceeds cookie size")
	}
	http.SetCookie(w, a.cookie(webSessionCookie, value, 7*24*3600, http.SameSiteStrictMode))
	return nil
}

func (a *webAuth) providers() (map[string]auth.Provider, error) {
	store, err := auth.Open()
	if err != nil {
		return nil, err
	}
	state, err := store.Load()
	if err != nil {
		return nil, err
	}
	providers := map[string]auth.Provider{}
	for name, provider := range state.Providers {
		if provider.Type == auth.ProviderBroker {
			providers[name] = provider
		}
	}
	return providers, nil
}

func (a *webAuth) registeredClient(ctx context.Context, discovery auth.OIDCDiscovery, redirect string) (string, error) {
	if discovery.RegistrationEndpoint == "" {
		return "", errors.New("broker dynamic client registration is unavailable")
	}
	endpoint, err := url.Parse(discovery.RegistrationEndpoint)
	issuer, issuerErr := url.Parse(discovery.Issuer)
	if err != nil || issuerErr != nil || endpoint.Scheme != issuer.Scheme || endpoint.Host != issuer.Host {
		return "", errors.New("broker registration endpoint has an unexpected origin")
	}
	key := discovery.Issuer + "\x00" + redirect
	a.mu.Lock()
	defer a.mu.Unlock()
	if id := a.clients[key]; id != "" {
		return id, nil
	}
	body, _ := json.Marshal(map[string]any{"redirect_uris": []string{redirect}, "token_endpoint_auth_method": "none", "grant_types": []string{"authorization_code", "refresh_token"}, "response_types": []string{"code"}, "application_type": "web", "client_name": "Graphit Code Web UI", "scope": "openid profile email offline_access"})
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, discovery.RegistrationEndpoint, strings.NewReader(string(body)))
	if err != nil {
		return "", err
	}
	req.Header.Set("Content-Type", "application/json")
	res, err := a.client.HTTP.Do(req)
	if err != nil {
		return "", err
	}
	defer res.Body.Close()
	if res.StatusCode != http.StatusCreated {
		return "", fmt.Errorf("broker client registration failed: HTTP %d", res.StatusCode)
	}
	var registration struct {
		ClientID string `json:"client_id"`
	}
	if err := json.NewDecoder(io.LimitReader(res.Body, 65536)).Decode(&registration); err != nil || registration.ClientID == "" {
		return "", errors.New("broker returned an invalid client registration")
	}
	a.clients[key] = registration.ClientID
	return registration.ClientID, nil
}

func (a *webAuth) login(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	var input struct {
		Provider string `json:"provider"`
	}
	if err := json.NewDecoder(http.MaxBytesReader(w, r.Body, 4096)).Decode(&input); err != nil {
		http.Error(w, "invalid login request", http.StatusBadRequest)
		return
	}
	providers, err := a.providers()
	provider, ok := providers[input.Provider]
	if err != nil || !ok {
		http.Error(w, "Broker provider unavailable", http.StatusBadRequest)
		return
	}
	origin, err := a.origin(r)
	if err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}
	resolved, err := auth.BrokerOIDCProvider(r.Context(), provider, a.client.HTTP)
	if err != nil {
		http.Error(w, "Broker discovery failed: "+err.Error(), http.StatusBadGateway)
		return
	}
	discovery, err := a.client.Discovery(r.Context(), resolved.OIDC.Issuer)
	if err != nil {
		http.Error(w, "Broker OIDC discovery failed", http.StatusBadGateway)
		return
	}
	redirect := origin + webCallbackPath
	clientID, err := a.registeredClient(r.Context(), discovery, redirect)
	if err != nil {
		http.Error(w, err.Error(), http.StatusBadGateway)
		return
	}
	copyOIDC := *resolved.OIDC
	copyOIDC.ClientID = clientID
	// A browser needs the Broker API audience, regardless of whether this Code
	// installation also exposes an MCP resource.
	copyOIDC.MCPResource = strings.TrimRight(discovery.Issuer, "/") + "/v1"
	resolved.OIDC = &copyOIDC
	request, err := a.client.AuthorizationRequest(resolved, discovery, redirect)
	if err != nil {
		http.Error(w, "could not start login", http.StatusInternalServerError)
		return
	}
	transaction := webLogin{Provider: provider.Name, Revision: provider.Revision, ClientID: clientID, Redirect: redirect, State: request.State, Nonce: request.Nonce, Verifier: request.Verifier, Issued: time.Now().Unix()}
	value, err := a.seal(transaction)
	if err != nil {
		http.Error(w, "could not start login", http.StatusInternalServerError)
		return
	}
	http.SetCookie(w, a.cookie(webLoginCookie, value, 600, http.SameSiteLaxMode))
	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(map[string]string{"authorization_url": request.URL})
}

func (a *webAuth) callback(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	cookie, err := r.Cookie(webLoginCookie)
	if err != nil {
		http.Error(w, "login transaction missing", http.StatusBadRequest)
		return
	}
	http.SetCookie(w, a.cookie(webLoginCookie, "", -1, http.SameSiteLaxMode))
	var transaction webLogin
	if err := a.open(cookie.Value, &transaction); err != nil || time.Since(time.Unix(transaction.Issued, 0)) > 10*time.Minute || transaction.Issued > time.Now().Unix() {
		http.Error(w, "login transaction expired", http.StatusBadRequest)
		return
	}
	if subtle.ConstantTimeCompare([]byte(transaction.State), []byte(r.URL.Query().Get("state"))) != 1 || r.URL.Query().Get("code") == "" || r.URL.Query().Get("error") != "" {
		http.Error(w, "invalid Broker login response", http.StatusBadRequest)
		return
	}
	origin, err := a.origin(r)
	if err != nil || transaction.Redirect != origin+webCallbackPath {
		http.Error(w, "login origin changed", http.StatusBadRequest)
		return
	}
	a.mu.Lock()
	for state, until := range a.used {
		if time.Now().After(until) {
			delete(a.used, state)
		}
	}
	if _, replay := a.used[transaction.State]; replay {
		a.mu.Unlock()
		http.Error(w, "login already used", http.StatusBadRequest)
		return
	}
	a.used[transaction.State] = time.Now().Add(10 * time.Minute)
	a.mu.Unlock()
	providers, err := a.providers()
	provider, ok := providers[transaction.Provider]
	if err != nil || !ok || provider.Revision != transaction.Revision {
		http.Error(w, "Broker provider changed", http.StatusBadRequest)
		return
	}
	resolved, err := auth.BrokerOIDCProvider(r.Context(), provider, a.client.HTTP)
	if err != nil {
		http.Error(w, "Broker discovery failed", http.StatusBadGateway)
		return
	}
	discovery, err := a.client.Discovery(r.Context(), resolved.OIDC.Issuer)
	if err != nil {
		http.Error(w, "Broker OIDC discovery failed", http.StatusBadGateway)
		return
	}
	copyOIDC := *resolved.OIDC
	copyOIDC.ClientID = transaction.ClientID
	copyOIDC.MCPResource = strings.TrimRight(discovery.Issuer, "/") + "/v1"
	resolved.OIDC = &copyOIDC
	profile, err := a.client.ExchangeCode(r.Context(), resolved, discovery, r.URL.Query().Get("code"), transaction.Verifier, transaction.Redirect, transaction.Nonce)
	if err != nil {
		http.Error(w, "Broker token exchange failed", http.StatusBadGateway)
		return
	}
	sessionID := make([]byte, 24)
	if _, err := rand.Read(sessionID); err != nil {
		http.Error(w, "could not create session", http.StatusInternalServerError)
		return
	}
	if err := a.setSession(w, webSession{ID: base64.RawURLEncoding.EncodeToString(sessionID), Expires: time.Now().Add(7 * 24 * time.Hour).Unix(), Provider: provider.Name, Revision: provider.Revision, ClientID: transaction.ClientID, Profile: profile}); err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}
	http.Redirect(w, r, "/workspace", http.StatusSeeOther)
}

func (a *webAuth) session(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	type providerInfo struct {
		Name string `json:"name"`
		Type string `json:"type"`
	}
	response := struct {
		Enabled       bool           `json:"enabled"`
		Authenticated bool           `json:"authenticated"`
		Username      string         `json:"username"`
		Provider      string         `json:"provider"`
		Providers     []providerInfo `json:"providers"`
	}{Enabled: a.enabled, Username: "Anônimo", Providers: []providerInfo{}}
	if a.enabled {
		providers, _ := a.providers()
		for name := range providers {
			response.Providers = append(response.Providers, providerInfo{Name: name, Type: "broker"})
		}
		sort.Slice(response.Providers, func(i, j int) bool { return response.Providers[i].Name < response.Providers[j].Name })
		if snapshot, ok := auth.RequestSnapshot(r.Context()); ok {
			response.Authenticated, response.Username, response.Provider = true, snapshot.Profile.Username, snapshot.Provider.Name
		}
	} else if snapshot, err := auth.ResolveActive(r.Context()); err == nil {
		response.Authenticated, response.Username, response.Provider = true, snapshot.Profile.Username, snapshot.Provider.Name
	}
	w.Header().Set("Content-Type", "application/json")
	w.Header().Set("Cache-Control", "no-store")
	_ = json.NewEncoder(w).Encode(response)
}

func (a *webAuth) logout(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	if cookie, err := r.Cookie(webSessionCookie); err == nil {
		var session webSession
		if a.open(cookie.Value, &session) == nil && session.Profile.OIDC != nil && session.Profile.OIDC.RefreshToken != "" {
			a.mu.Lock()
			if recent, ok := a.refreshed[sha256.Sum256([]byte(cookie.Value))]; ok {
				session = recent
			}
			a.mu.Unlock()
			providers, loadErr := a.providers()
			provider, ok := providers[session.Provider]
			if loadErr == nil && ok && provider.Revision == session.Revision {
				resolved, resolveErr := auth.BrokerOIDCProvider(r.Context(), provider, a.client.HTTP)
				if resolveErr != nil {
					http.Error(w, "Broker session revocation unavailable", http.StatusBadGateway)
					return
				}
				discovery, discoverErr := a.client.Discovery(r.Context(), resolved.OIDC.Issuer)
				if discoverErr != nil || discovery.RevocationEndpoint == "" {
					http.Error(w, "Broker revocation endpoint unavailable", http.StatusBadGateway)
					return
				}
				endpoint, parseErr := url.Parse(discovery.RevocationEndpoint)
				issuer, issuerErr := url.Parse(discovery.Issuer)
				if parseErr != nil || issuerErr != nil || endpoint.Scheme != issuer.Scheme || endpoint.Host != issuer.Host {
					http.Error(w, "Broker revocation origin mismatch", http.StatusBadGateway)
					return
				}
				form := url.Values{"token": {session.Profile.OIDC.RefreshToken}, "token_type_hint": {"refresh_token"}, "client_id": {session.ClientID}}
				req, reqErr := http.NewRequestWithContext(r.Context(), http.MethodPost, discovery.RevocationEndpoint, strings.NewReader(form.Encode()))
				if reqErr != nil {
					http.Error(w, "Broker revocation request failed", http.StatusBadGateway)
					return
				}
				req.Header.Set("Content-Type", "application/x-www-form-urlencoded")
				res, revokeErr := a.client.HTTP.Do(req)
				if revokeErr != nil {
					http.Error(w, "Broker session revocation failed", http.StatusBadGateway)
					return
				}
				_ = res.Body.Close()
				if res.StatusCode != http.StatusOK {
					http.Error(w, "Broker session revocation failed", http.StatusBadGateway)
					return
				}
			}
		}
		if session.ID != "" {
			a.mu.Lock()
			for key, refreshed := range a.refreshed {
				if refreshed.ID == session.ID {
					delete(a.refreshed, key)
				}
			}
			a.mu.Unlock()
		}
	}
	http.SetCookie(w, a.cookie(webSessionCookie, "", -1, http.SameSiteStrictMode))
	w.Header().Set("Content-Type", "application/json")
	_, _ = w.Write([]byte(`{"ok":true}`))
}

func (a *webAuth) requestSession(w http.ResponseWriter, r *http.Request) (auth.Snapshot, bool) {
	cookie, err := r.Cookie(webSessionCookie)
	if err != nil {
		return auth.Snapshot{}, false
	}
	var session webSession
	if err := a.open(cookie.Value, &session); err != nil || session.Profile.OIDC == nil || session.ClientID == "" || session.ID == "" || time.Now().Unix() >= session.Expires {
		return auth.Snapshot{}, false
	}
	providers, err := a.providers()
	provider, ok := providers[session.Provider]
	if err != nil || !ok || provider.Revision != session.Revision {
		return auth.Snapshot{}, false
	}
	if time.Until(session.Profile.OIDC.ExpiresAt) <= 2*time.Minute {
		digest := sha256.Sum256([]byte(cookie.Value))
		a.mu.Lock()
		if recent, ok := a.refreshed[digest]; ok {
			session = recent
		} else {
			resolved, resolveErr := auth.BrokerOIDCProvider(r.Context(), provider, a.client.HTTP)
			if resolveErr != nil {
				a.mu.Unlock()
				return auth.Snapshot{}, false
			}
			copyOIDC := *resolved.OIDC
			copyOIDC.ClientID = session.ClientID
			copyOIDC.MCPResource = strings.TrimRight(copyOIDC.Issuer, "/") + "/v1"
			resolved.OIDC = &copyOIDC
			refreshed, refreshErr := a.client.Refresh(r.Context(), resolved, session.Profile)
			if refreshErr != nil {
				a.mu.Unlock()
				return auth.Snapshot{}, false
			}
			session.Profile = refreshed
			a.refreshed[digest] = session
			if len(a.refreshed) > 256 {
				a.refreshed = map[[32]byte]webSession{digest: session}
			}
		}
		a.mu.Unlock()
		if err := a.setSession(w, session); err != nil {
			return auth.Snapshot{}, false
		}
	}
	if !time.Now().Before(session.Profile.OIDC.ExpiresAt) {
		return auth.Snapshot{}, false
	}
	return auth.Snapshot{Provider: provider, Profile: session.Profile}, true
}

func (a *webAuth) wrap(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if !a.enabled {
			next.ServeHTTP(w, r)
			return
		}
		if strings.HasPrefix(r.URL.Path, "/api/") && r.URL.Path != webCallbackPath {
			if origin := r.Header.Get("Origin"); origin != "" {
				expected, err := a.origin(r)
				if err != nil || origin != expected {
					http.Error(w, "origin rejected", http.StatusForbidden)
					return
				}
			}
			if r.Method != http.MethodGet && r.Method != http.MethodHead && r.Method != http.MethodOptions && r.Header.Get("X-Graphit-Request") != "ui" {
				http.Error(w, "CSRF check failed", http.StatusForbidden)
				return
			}
			if r.URL.Path != "/api/auth/login" && r.URL.Path != "/api/auth/session" && r.URL.Path != "/api/auth/logout" {
				snapshot, ok := a.requestSession(w, r)
				if !ok {
					http.Error(w, "authentication required", http.StatusUnauthorized)
					return
				}
				ctx := auth.WithRequestSnapshot(r.Context(), snapshot)
				ctx = auth.WithBrokerBearer(ctx, snapshot.Profile.OIDC.AccessToken)
				next.ServeHTTP(w, r.WithContext(ctx))
				return
			}
		}
		if r.URL.Path == "/api/auth/session" && a.enabled {
			if snapshot, ok := a.requestSession(w, r); ok {
				r = r.WithContext(auth.WithRequestSnapshot(r.Context(), snapshot))
			}
		}
		next.ServeHTTP(w, r)
	})
}
