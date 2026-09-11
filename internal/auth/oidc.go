package auth

import (
	"context"
	"crypto"
	"crypto/ecdsa"
	"crypto/ed25519"
	"crypto/elliptic"
	"crypto/rand"
	"crypto/rsa"
	"crypto/sha256"
	"crypto/sha512"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"math/big"
	"net"
	"net/http"
	"net/url"
	"strconv"
	"strings"
	"time"
)

type OIDCDiscovery struct {
	Issuer                            string   `json:"issuer"`
	AuthorizationEndpoint             string   `json:"authorization_endpoint"`
	TokenEndpoint                     string   `json:"token_endpoint"`
	JWKSURI                           string   `json:"jwks_uri"`
	UserinfoEndpoint                  string   `json:"userinfo_endpoint,omitempty"`
	ScopesSupported                   []string `json:"scopes_supported,omitempty"`
	GrantTypesSupported               []string `json:"grant_types_supported,omitempty"`
	CodeChallengeMethodsSupported     []string `json:"code_challenge_methods_supported,omitempty"`
	TokenEndpointAuthMethodsSupported []string `json:"token_endpoint_auth_methods_supported,omitempty"`
	IDTokenSigningAlgorithmsSupported []string `json:"id_token_signing_alg_values_supported,omitempty"`
}

type OIDCClient struct {
	HTTP *http.Client
	Now  func() time.Time
}

func NewOIDCClient() *OIDCClient { return &OIDCClient{HTTP: http.DefaultClient, Now: time.Now} }

// LoginInteractive performs the native-app Authorization Code + PKCE flow through a
// loopback callback. Callers decide how to open the URL, which keeps UI policy outside
// the protocol implementation.
func (c *OIDCClient) LoginInteractive(ctx context.Context, provider Provider, openURL func(string) error) (Profile, error) {
	if provider.OIDC == nil {
		return Profile{}, errors.New("provider is not OIDC")
	}
	discovery, err := c.Discovery(ctx, provider.OIDC.Issuer)
	if err != nil {
		return Profile{}, err
	}
	redirectURI := strings.TrimSpace(provider.OIDC.RedirectURI)
	listenAddress := "127.0.0.1:0"
	callbackPath := "/callback"
	if path := strings.TrimSpace(provider.OIDC.RedirectURIPath); path != "" {
		if !strings.HasPrefix(path, "/") || strings.ContainsAny(path, "?#") {
			return Profile{}, errors.New("OIDC redirect URI path must be an absolute path without query or fragment")
		}
		callbackPath = path
	}
	if redirectURI != "" {
		u, err := url.Parse(redirectURI)
		if err != nil || u.Scheme != "http" || (u.Hostname() != "127.0.0.1" && u.Hostname() != "localhost" && u.Hostname() != "::1") || u.Port() == "" {
			return Profile{}, errors.New("OIDC redirect URI must be an HTTP loopback URL with an explicit port")
		}
		listenAddress, callbackPath = u.Host, u.Path
		if callbackPath == "" {
			callbackPath = "/"
		}
	}
	listener, err := net.Listen("tcp", listenAddress)
	if err != nil {
		return Profile{}, fmt.Errorf("listen for OIDC callback: %w", err)
	}
	defer listener.Close()
	if redirectURI == "" {
		redirectURI = "http://" + listener.Addr().String() + callbackPath
	}
	authRequest, err := c.AuthorizationRequest(provider, discovery, redirectURI)
	if err != nil {
		return Profile{}, err
	}
	type callbackResult struct{ code, state, protocolError string }
	result := make(chan callbackResult, 1)
	mux := http.NewServeMux()
	mux.HandleFunc(callbackPath, func(w http.ResponseWriter, r *http.Request) {
		select {
		case result <- callbackResult{code: r.URL.Query().Get("code"), state: r.URL.Query().Get("state"), protocolError: r.URL.Query().Get("error")}:
		default:
		}
		w.Header().Set("Content-Type", "text/plain; charset=utf-8")
		_, _ = io.WriteString(w, "Authentication received. You may close this window.\n")
	})
	server := &http.Server{Handler: mux, ReadHeaderTimeout: 5 * time.Second}
	defer server.Close()
	go func() { _ = server.Serve(listener) }()
	if openURL == nil {
		return Profile{}, errors.New("OIDC login requires a browser opener")
	}
	if err := openURL(authRequest.URL); err != nil {
		return Profile{}, fmt.Errorf("open OIDC authorization URL: %w", err)
	}
	select {
	case <-ctx.Done():
		return Profile{}, fmt.Errorf("OIDC login callback: %w", ctx.Err())
	case callback := <-result:
		if callback.protocolError != "" {
			return Profile{}, fmt.Errorf("OIDC authorization failed: %s", callback.protocolError)
		}
		if callback.state != authRequest.State {
			return Profile{}, errors.New("OIDC callback state does not match login request")
		}
		if callback.code == "" {
			return Profile{}, errors.New("OIDC callback did not include an authorization code")
		}
		return c.ExchangeCode(ctx, provider, discovery, callback.code, authRequest.Verifier, redirectURI, authRequest.Nonce)
	}
}

func (c *OIDCClient) Discovery(ctx context.Context, issuer string) (OIDCDiscovery, error) {
	issuer = strings.TrimRight(strings.TrimSpace(issuer), "/")
	var discovery OIDCDiscovery
	if err := c.getJSON(ctx, issuer+"/.well-known/openid-configuration", &discovery); err != nil {
		return discovery, fmt.Errorf("OIDC discovery: %w", err)
	}
	if strings.TrimRight(discovery.Issuer, "/") != issuer {
		return discovery, errors.New("OIDC discovery issuer does not match configured issuer")
	}
	if discovery.AuthorizationEndpoint == "" || discovery.TokenEndpoint == "" || discovery.JWKSURI == "" {
		return discovery, errors.New("OIDC discovery document is incomplete")
	}
	return discovery, nil
}

type AuthorizationRequest struct {
	URL, State, Nonce, Verifier string
}

func (c *OIDCClient) AuthorizationRequest(provider Provider, discovery OIDCDiscovery, redirectURI string) (AuthorizationRequest, error) {
	if provider.OIDC == nil {
		return AuthorizationRequest{}, errors.New("provider is not OIDC")
	}
	state, err := randomURLSafe(32)
	if err != nil {
		return AuthorizationRequest{}, err
	}
	nonce, err := randomURLSafe(32)
	if err != nil {
		return AuthorizationRequest{}, err
	}
	verifier, err := randomURLSafe(48)
	if err != nil {
		return AuthorizationRequest{}, err
	}
	challenge := sha256.Sum256([]byte(verifier))
	scopes := append([]string(nil), provider.OIDC.Scopes...)
	if !contains(scopes, "openid") {
		scopes = append([]string{"openid"}, scopes...)
	}
	values := url.Values{
		"response_type": {"code"}, "client_id": {provider.OIDC.ClientID}, "redirect_uri": {redirectURI},
		"scope": {strings.Join(scopes, " ")}, "state": {state}, "nonce": {nonce},
		"code_challenge": {base64.RawURLEncoding.EncodeToString(challenge[:])}, "code_challenge_method": {"S256"},
	}
	audience, resource := provider.MCP.Audience, provider.MCP.Resource
	// In relay mode the login token is minted for the shared MCP/broker audience.
	// In token-exchange mode it must remain a token for MCP; a request-scoped RFC
	// 8693 exchange obtains the distinct broker token later.
	if provider.Broker != nil && provider.Broker.TokenStrategy != "token-exchange" {
		if provider.Broker.Audience != "" {
			audience = provider.Broker.Audience
		}
		if provider.Broker.Resource != "" {
			resource = provider.Broker.Resource
		}
	}
	if audience != "" {
		values.Set("audience", audience)
	}
	if resource != "" {
		values.Set("resource", resource)
	}
	for key, value := range provider.OIDC.AuthParams {
		values.Set(key, value)
	}
	return AuthorizationRequest{URL: discovery.AuthorizationEndpoint + "?" + values.Encode(), State: state, Nonce: nonce, Verifier: verifier}, nil
}

type tokenResponse struct {
	AccessToken      string `json:"access_token"`
	RefreshToken     string `json:"refresh_token"`
	IDToken          string `json:"id_token"`
	TokenType        string `json:"token_type"`
	ExpiresIn        int64  `json:"expires_in"`
	Error            string `json:"error"`
	ErrorDescription string `json:"error_description"`
}

type VerifiedIdentity struct {
	Issuer       string
	Subject      string
	Username     string
	Organization string
	Teams        []string
}

type ExchangedAccessToken struct {
	AccessToken string
	ExpiresAt   time.Time
}

// VerifyAccessToken validates a JWT access token against provider discovery/JWKS and maps only
// claims from that verified token. It is used by Streamable HTTP MCP before request context exists.
func (c *OIDCClient) VerifyAccessToken(ctx context.Context, provider Provider, raw, audience string) (VerifiedIdentity, error) {
	if provider.OIDC == nil {
		return VerifiedIdentity{}, errors.New("provider is not OIDC")
	}
	if strings.TrimSpace(raw) == "" || strings.TrimSpace(audience) == "" {
		return VerifiedIdentity{}, errors.New("access token and MCP audience are required")
	}
	discovery, err := c.Discovery(ctx, provider.OIDC.Issuer)
	if err != nil {
		return VerifiedIdentity{}, err
	}
	claims, err := c.verifySignedToken(ctx, discovery, audience, raw, "", "access token")
	if err != nil {
		return VerifiedIdentity{}, fmt.Errorf("verify access token: %w", err)
	}
	username, err := stringClaim(claims, provider.OIDC.UsernameClaim, true)
	if err != nil {
		return VerifiedIdentity{}, err
	}
	organization, err := stringClaim(claims, provider.OIDC.OrganizationClaim, false)
	if err != nil {
		return VerifiedIdentity{}, err
	}
	teams, err := stringSliceClaim(claims, provider.OIDC.TeamsClaim)
	if err != nil {
		return VerifiedIdentity{}, err
	}
	issuer, _ := claims["iss"].(string)
	subject, _ := claims["sub"].(string)
	if subject == "" {
		return VerifiedIdentity{}, errors.New("verified access token has no subject")
	}
	return VerifiedIdentity{Issuer: issuer, Subject: subject, Username: username, Organization: organization, Teams: teams}, nil
}

// ExchangeAccessToken performs OAuth 2.0 Token Exchange (RFC 8693) for a broker-scoped token.
func (c *OIDCClient) ExchangeAccessToken(ctx context.Context, provider Provider, subjectToken string) (ExchangedAccessToken, error) {
	if provider.OIDC == nil || provider.Broker == nil {
		return ExchangedAccessToken{}, errors.New("OIDC provider with broker configuration is required")
	}
	endpoint := strings.TrimSpace(provider.Broker.TokenExchangeEndpoint)
	if endpoint == "" {
		discovery, err := c.Discovery(ctx, provider.OIDC.Issuer)
		if err != nil {
			return ExchangedAccessToken{}, err
		}
		endpoint = discovery.TokenEndpoint
	}
	form := url.Values{
		"grant_type":           {"urn:ietf:params:oauth:grant-type:token-exchange"},
		"subject_token":        {subjectToken},
		"subject_token_type":   {"urn:ietf:params:oauth:token-type:access_token"},
		"requested_token_type": {"urn:ietf:params:oauth:token-type:access_token"},
		"client_id":            {provider.OIDC.ClientID},
	}
	if provider.Broker.Audience != "" {
		form.Set("audience", provider.Broker.Audience)
	}
	if provider.Broker.Resource != "" {
		form.Set("resource", provider.Broker.Resource)
	}
	token, err := c.token(ctx, endpoint, form, provider.OIDC)
	if err != nil {
		return ExchangedAccessToken{}, fmt.Errorf("exchange broker access token: %w", err)
	}
	if token.AccessToken == "" || token.ExpiresIn <= 0 || (token.TokenType != "" && !strings.EqualFold(token.TokenType, "Bearer")) {
		return ExchangedAccessToken{}, errors.New("token exchange response must contain a bearer access_token with positive expires_in")
	}
	return ExchangedAccessToken{AccessToken: token.AccessToken, ExpiresAt: c.now().Add(time.Duration(token.ExpiresIn) * time.Second)}, nil
}

func (c *OIDCClient) ExchangeCode(ctx context.Context, provider Provider, discovery OIDCDiscovery, code, verifier, redirectURI, nonce string) (Profile, error) {
	form := url.Values{"grant_type": {"authorization_code"}, "code": {code}, "client_id": {provider.OIDC.ClientID}, "redirect_uri": {redirectURI}, "code_verifier": {verifier}}
	token, err := c.token(ctx, discovery.TokenEndpoint, form, provider.OIDC)
	if err != nil {
		return Profile{}, err
	}
	return c.profileFromToken(ctx, provider, discovery, token, nonce)
}

func (c *OIDCClient) LoginWithTokens(ctx context.Context, provider Provider, accessToken, refreshToken, idToken string, expiresAt time.Time) (Profile, error) {
	discovery, err := c.Discovery(ctx, provider.OIDC.Issuer)
	if err != nil {
		return Profile{}, err
	}
	return c.profileFromToken(ctx, provider, discovery, tokenResponse{AccessToken: accessToken, RefreshToken: refreshToken, IDToken: idToken, TokenType: "Bearer", ExpiresIn: secondsUntil(c.now(), expiresAt)}, "")
}

func (c *OIDCClient) Refresh(ctx context.Context, provider Provider, profile Profile) (Profile, error) {
	if profile.OIDC == nil || profile.OIDC.RefreshToken == "" {
		return Profile{}, errors.New("OIDC session expired and has no refresh token")
	}
	session := *profile.OIDC
	discovery, err := c.Discovery(ctx, provider.OIDC.Issuer)
	if err != nil {
		return Profile{}, err
	}
	form := url.Values{"grant_type": {"refresh_token"}, "refresh_token": {session.RefreshToken}, "client_id": {provider.OIDC.ClientID}}
	if len(provider.OIDC.Scopes) > 0 {
		form.Set("scope", strings.Join(provider.OIDC.Scopes, " "))
	}
	token, err := c.token(ctx, discovery.TokenEndpoint, form, provider.OIDC)
	if err != nil {
		return Profile{}, err
	}
	if provider.Type == ProviderBroker && (token.RefreshToken == "" || token.RefreshToken == session.RefreshToken) {
		return Profile{}, errors.New("broker did not rotate the refresh token")
	}
	if token.RefreshToken == "" {
		token.RefreshToken = session.RefreshToken
	}
	if token.IDToken == "" {
		profile.OIDC = &OIDCSession{AccessToken: token.AccessToken, RefreshToken: token.RefreshToken, IDToken: session.IDToken, TokenType: token.TokenType, ExpiresAt: c.now().Add(time.Duration(token.ExpiresIn) * time.Second)}
		return profile, nil
	}
	return c.profileFromToken(ctx, provider, discovery, token, "")
}

func (c *OIDCClient) profileFromToken(ctx context.Context, provider Provider, discovery OIDCDiscovery, token tokenResponse, nonce string) (Profile, error) {
	if token.AccessToken == "" || token.IDToken == "" {
		return Profile{}, errors.New("OIDC token response must include access_token and id_token")
	}
	claims, err := c.verifySignedToken(ctx, discovery, provider.OIDC.ClientID, token.IDToken, nonce, "ID token")
	if err != nil {
		return Profile{}, err
	}
	username, err := stringClaim(claims, provider.OIDC.UsernameClaim, true)
	if err != nil {
		return Profile{}, err
	}
	organization, err := stringClaim(claims, provider.OIDC.OrganizationClaim, false)
	if err != nil {
		return Profile{}, err
	}
	teams, err := stringSliceClaim(claims, provider.OIDC.TeamsClaim)
	if err != nil {
		return Profile{}, err
	}
	issuer, _ := claims["iss"].(string)
	subject, _ := claims["sub"].(string)
	if subject == "" {
		return Profile{}, errors.New("verified ID token has no subject")
	}
	return Profile{Provider: provider.Name, ProviderRevision: provider.Revision, Issuer: issuer, Subject: subject, Username: username, Organization: organization, Teams: teams,
		OIDC: &OIDCSession{AccessToken: token.AccessToken, RefreshToken: token.RefreshToken, IDToken: token.IDToken, TokenType: token.TokenType, ExpiresAt: c.now().Add(time.Duration(token.ExpiresIn) * time.Second)}}, nil
}

func (c *OIDCClient) token(ctx context.Context, endpoint string, form url.Values, oidc *OIDCConfig) (tokenResponse, error) {
	var token tokenResponse
	if oidc != nil && oidc.ClientSecret != "" && oidc.TokenAuthMethod != "client_secret_basic" && oidc.TokenAuthMethod != "none" {
		form.Set("client_secret", oidc.ClientSecret)
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, endpoint, strings.NewReader(form.Encode()))
	if err != nil {
		return token, err
	}
	req.Header.Set("Content-Type", "application/x-www-form-urlencoded")
	if oidc != nil && oidc.ClientSecret != "" {
		switch oidc.TokenAuthMethod {
		case "client_secret_basic":
			req.SetBasicAuth(oidc.ClientID, oidc.ClientSecret)
		}
	}
	resp, err := c.client().Do(req)
	if err != nil {
		return token, fmt.Errorf("OIDC token request: %w", err)
	}
	defer resp.Body.Close()
	if err := decodeLimited(resp.Body, &token); err != nil {
		return token, fmt.Errorf("OIDC token response: %w", err)
	}
	if resp.StatusCode/100 != 2 || token.Error != "" {
		return token, fmt.Errorf("OIDC token request failed: %s", firstNonEmpty(token.ErrorDescription, token.Error, resp.Status))
	}
	return token, nil
}

type jwkSet struct {
	Keys []json.RawMessage `json:"keys"`
}
type jwk struct{ Kty, Kid, Alg, Use, N, E, Crv, X, Y string }

func (c *OIDCClient) verifySignedToken(ctx context.Context, discovery OIDCDiscovery, clientID, raw, nonce, kind string) (map[string]any, error) {
	parts := strings.Split(raw, ".")
	if len(parts) != 3 {
		return nil, fmt.Errorf("invalid %s format", kind)
	}
	decode := func(s string) ([]byte, error) { return base64.RawURLEncoding.DecodeString(s) }
	headerBytes, err := decode(parts[0])
	if err != nil {
		return nil, fmt.Errorf("invalid %s header", kind)
	}
	claimBytes, err := decode(parts[1])
	if err != nil {
		return nil, fmt.Errorf("invalid %s claims", kind)
	}
	signature, err := decode(parts[2])
	if err != nil {
		return nil, fmt.Errorf("invalid %s signature", kind)
	}
	var header struct{ Alg, Kid string }
	if json.Unmarshal(headerBytes, &header) != nil || header.Alg == "" || header.Alg == "none" {
		return nil, fmt.Errorf("invalid %s algorithm", kind)
	}
	var set jwkSet
	if err := c.getJSON(ctx, discovery.JWKSURI, &set); err != nil {
		return nil, fmt.Errorf("OIDC JWKS: %w", err)
	}
	verified := false
	for _, rawKey := range set.Keys {
		var key jwk
		if json.Unmarshal(rawKey, &key) != nil || (header.Kid != "" && key.Kid != header.Kid) || (key.Alg != "" && key.Alg != header.Alg) {
			continue
		}
		if verifyJWK(key, header.Alg, []byte(parts[0]+"."+parts[1]), signature) == nil {
			verified = true
			break
		}
	}
	if !verified {
		return nil, fmt.Errorf("%s signature verification failed", kind)
	}
	var claims map[string]any
	if err := json.Unmarshal(claimBytes, &claims); err != nil {
		return nil, fmt.Errorf("invalid %s claims", kind)
	}
	if iss, _ := claims["iss"].(string); strings.TrimRight(iss, "/") != strings.TrimRight(discovery.Issuer, "/") {
		return nil, fmt.Errorf("%s issuer does not match provider", kind)
	}
	if !audienceContains(claims["aud"], clientID) {
		return nil, fmt.Errorf("%s audience does not include the required audience", kind)
	}
	exp, ok := numberClaim(claims["exp"])
	if !ok || c.now().Unix() >= exp {
		return nil, fmt.Errorf("%s is expired", kind)
	}
	if nbf, ok := numberClaim(claims["nbf"]); ok && c.now().Unix() < nbf {
		return nil, fmt.Errorf("%s is not active yet", kind)
	}
	if nonce != "" {
		got, _ := claims["nonce"].(string)
		if got != nonce {
			return nil, fmt.Errorf("%s nonce does not match login request", kind)
		}
	}
	return claims, nil
}

func verifyJWK(key jwk, alg string, message, signature []byte) error {
	if alg == "EdDSA" {
		if key.Kty != "OKP" || key.Crv != "Ed25519" {
			return errors.New("unsupported EdDSA key")
		}
		publicKey, err := base64.RawURLEncoding.DecodeString(key.X)
		if err != nil || len(publicKey) != ed25519.PublicKeySize || !ed25519.Verify(ed25519.PublicKey(publicKey), message, signature) {
			return errors.New("invalid EdDSA signature")
		}
		return nil
	}
	var hash crypto.Hash
	switch alg {
	case "RS256", "ES256":
		hash = crypto.SHA256
	case "RS384", "ES384":
		hash = crypto.SHA384
	case "RS512", "ES512":
		hash = crypto.SHA512
	default:
		return errors.New("unsupported signing algorithm")
	}
	var digest []byte
	switch hash {
	case crypto.SHA256:
		h := sha256.Sum256(message)
		digest = h[:]
	case crypto.SHA384:
		h := sha512.Sum384(message)
		digest = h[:]
	case crypto.SHA512:
		h := sha512.Sum512(message)
		digest = h[:]
	}
	switch key.Kty {
	case "RSA":
		nb, err := base64.RawURLEncoding.DecodeString(key.N)
		if err != nil {
			return err
		}
		eb, err := base64.RawURLEncoding.DecodeString(key.E)
		if err != nil {
			return err
		}
		e := 0
		for _, b := range eb {
			e = e<<8 + int(b)
		}
		if e == 0 {
			return errors.New("invalid RSA exponent")
		}
		return rsa.VerifyPKCS1v15(&rsa.PublicKey{N: new(big.Int).SetBytes(nb), E: e}, hash, digest, signature)
	case "EC":
		var curve elliptic.Curve
		var size int
		switch key.Crv {
		case "P-256":
			curve, size = elliptic.P256(), 32
		case "P-384":
			curve, size = elliptic.P384(), 48
		case "P-521":
			curve, size = elliptic.P521(), 66
		default:
			return errors.New("unsupported EC curve")
		}
		if len(signature) != size*2 {
			return errors.New("invalid ECDSA signature")
		}
		xb, _ := base64.RawURLEncoding.DecodeString(key.X)
		yb, _ := base64.RawURLEncoding.DecodeString(key.Y)
		if !ecdsa.Verify(&ecdsa.PublicKey{Curve: curve, X: new(big.Int).SetBytes(xb), Y: new(big.Int).SetBytes(yb)}, digest, new(big.Int).SetBytes(signature[:size]), new(big.Int).SetBytes(signature[size:])) {
			return errors.New("invalid ECDSA signature")
		}
		return nil
	default:
		return errors.New("unsupported JWK key type")
	}
}

func (c *OIDCClient) getJSON(ctx context.Context, target string, out any) error {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, target, nil)
	if err != nil {
		return err
	}
	resp, err := c.client().Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	if resp.StatusCode/100 != 2 {
		return fmt.Errorf("HTTP %s", resp.Status)
	}
	return decodeLimited(resp.Body, out)
}
func (c *OIDCClient) client() *http.Client {
	if c.HTTP != nil {
		return c.HTTP
	}
	return http.DefaultClient
}
func (c *OIDCClient) now() time.Time {
	if c.Now != nil {
		return c.Now()
	}
	return time.Now()
}
func decodeLimited(r io.Reader, out any) error {
	return json.NewDecoder(io.LimitReader(r, 2<<20)).Decode(out)
}
func randomURLSafe(n int) (string, error) {
	b := make([]byte, n)
	if _, err := rand.Read(b); err != nil {
		return "", err
	}
	return base64.RawURLEncoding.EncodeToString(b), nil
}
func contains(items []string, want string) bool {
	for _, v := range items {
		if v == want {
			return true
		}
	}
	return false
}
func firstNonEmpty(values ...string) string {
	for _, v := range values {
		if v != "" {
			return v
		}
	}
	return "unknown error"
}
func secondsUntil(now, then time.Time) int64 {
	if then.IsZero() {
		return 3600
	}
	n := int64(then.Sub(now).Seconds())
	if n < 0 {
		return 0
	}
	return n
}
func numberClaim(v any) (int64, bool) {
	switch n := v.(type) {
	case float64:
		return int64(n), true
	case json.Number:
		i, e := n.Int64()
		return i, e == nil
	case string:
		i, e := strconv.ParseInt(n, 10, 64)
		return i, e == nil
	}
	return 0, false
}
func audienceContains(v any, want string) bool {
	switch a := v.(type) {
	case string:
		return a == want
	case []any:
		for _, v := range a {
			if s, ok := v.(string); ok && s == want {
				return true
			}
		}
	}
	return false
}
func stringClaim(claims map[string]any, name string, required bool) (string, error) {
	if name == "" {
		if required {
			return "", errors.New("claim mapping is required")
		}
		return "", nil
	}
	v, ok := claimValue(claims, name)
	if !ok {
		if required {
			return "", fmt.Errorf("verified ID token has no %q claim", name)
		}
		return "", nil
	}
	s, ok := v.(string)
	if !ok || strings.TrimSpace(s) == "" {
		return "", fmt.Errorf("verified ID token claim %q must be a non-empty string", name)
	}
	return s, nil
}
func stringSliceClaim(claims map[string]any, name string) ([]string, error) {
	if name == "" {
		return nil, nil
	}
	v, ok := claimValue(claims, name)
	if !ok {
		return nil, nil
	}
	switch x := v.(type) {
	case string:
		if x == "" {
			return nil, nil
		}
		return []string{x}, nil
	case []any:
		out := make([]string, 0, len(x))
		for _, item := range x {
			s, ok := item.(string)
			if !ok {
				return nil, fmt.Errorf("verified ID token claim %q must contain only strings", name)
			}
			out = append(out, s)
		}
		return out, nil
	default:
		return nil, fmt.Errorf("verified ID token claim %q must be a string or string array", name)
	}
}

// claimValue keeps exact namespaced claims working and otherwise permits dotted traversal for
// IdPs that group custom claims in nested objects.
func claimValue(claims map[string]any, name string) (any, bool) {
	if value, ok := claims[name]; ok {
		return value, true
	}
	var current any = claims
	for _, segment := range strings.Split(name, ".") {
		object, ok := current.(map[string]any)
		if !ok {
			return nil, false
		}
		current, ok = object[segment]
		if !ok {
			return nil, false
		}
	}
	return current, true
}
