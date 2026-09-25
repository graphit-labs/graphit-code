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

	"github.com/theory/jsonpath"
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
		callback := callbackResult{code: r.URL.Query().Get("code"), state: r.URL.Query().Get("state"), protocolError: r.URL.Query().Get("error")}
		select {
		case result <- callback:
		default:
		}
		writeOIDCCallbackPage(w, callback.protocolError == "" && callback.state == authRequest.State && callback.code != "")
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
	issuer = strings.TrimSpace(issuer)
	var discovery OIDCDiscovery
	if err := c.getJSON(ctx, strings.TrimRight(issuer, "/")+"/.well-known/openid-configuration", &discovery); err != nil {
		return discovery, fmt.Errorf("OIDC discovery: %w", err)
	}
	if discovery.Issuer != issuer {
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
	var nonce string
	if provider.OIDC.UseNonce() {
		nonce, err = randomURLSafe(32)
		if err != nil {
			return AuthorizationRequest{}, err
		}
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
		"scope": {strings.Join(scopes, " ")}, "state": {state},
		"code_challenge": {base64.RawURLEncoding.EncodeToString(challenge[:])}, "code_challenge_method": {"S256"},
	}
	if nonce != "" {
		values.Set("nonce", nonce)
	}
	if provider.OIDC.MCPResource != "" {
		values.Set("resource", provider.OIDC.MCPResource)
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
	// ExpiresAt is the verified token's own expiry. Verification already rejects an
	// expired token, so this is not a second gate: it is what a bearer middleware needs
	// to report the session's remaining lifetime without parsing the token again.
	ExpiresAt time.Time
}

// VerifyAccessToken validates a JWT access token against provider discovery/JWKS and maps only
// claims from that verified token. It is used by Streamable HTTP MCP before request context exists.
func (c *OIDCClient) VerifyAccessToken(ctx context.Context, provider Provider, raw string, audiences []string) (VerifiedIdentity, error) {
	if provider.OIDC == nil {
		return VerifiedIdentity{}, errors.New("provider is not OIDC")
	}
	accepted := make([]string, 0, len(audiences))
	for _, audience := range audiences {
		if trimmed := strings.TrimSpace(audience); trimmed != "" {
			accepted = append(accepted, trimmed)
		}
	}
	if strings.TrimSpace(raw) == "" {
		return VerifiedIdentity{}, errors.New("access token is required")
	}
	if provider.Type != ProviderBroker || len(accepted) != 1 {
		return VerifiedIdentity{}, errors.New("one canonical MCP resource audience is required for a broker provider")
	}
	discovery, err := c.Discovery(ctx, provider.OIDC.Issuer)
	if err != nil {
		return VerifiedIdentity{}, err
	}
	claims, err := c.verifySignedToken(ctx, discovery, accepted, raw, "", "Broker access token", true)
	if err != nil {
		return VerifiedIdentity{}, fmt.Errorf("verify access token: %w", err)
	}
	if claims["token_use"] != "access" {
		return VerifiedIdentity{}, errors.New("verified token is not an access token")
	}
	if clientID, ok := claims["client_id"].(string); !ok || clientID == "" {
		return VerifiedIdentity{}, errors.New("broker access token has no client_id")
	}
	if _, ok := claims["iat"].(float64); !ok {
		return VerifiedIdentity{}, errors.New("broker access token has no numeric issued-at time")
	}
	if _, ok := claims["exp"].(float64); !ok {
		return VerifiedIdentity{}, errors.New("broker access token has no numeric expiration")
	}
	if jti, ok := claims["jti"].(string); !ok || jti == "" {
		return VerifiedIdentity{}, errors.New("broker access token has no token identifier")
	}
	// A registered MCP client has its own client_id, distinct from graphit-cli.
	// The Broker's UserInfo endpoint checks its grant and revocation state.
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
	var expiresAt time.Time
	if exp, ok := numberClaim(claims["exp"]); ok {
		expiresAt = time.Unix(exp, 0)
	}
	return VerifiedIdentity{Issuer: issuer, Subject: subject, Username: username, Organization: organization, Teams: teams, ExpiresAt: expiresAt}, nil
}

func (c *OIDCClient) ExchangeCode(ctx context.Context, provider Provider, discovery OIDCDiscovery, code, verifier, redirectURI, nonce string) (Profile, error) {
	form := url.Values{"grant_type": {"authorization_code"}, "code": {code}, "client_id": {provider.OIDC.ClientID}, "redirect_uri": {redirectURI}, "code_verifier": {verifier}}
	if provider.OIDC.MCPResource != "" {
		form.Set("resource", provider.OIDC.MCPResource)
	}
	token, err := c.token(ctx, discovery.TokenEndpoint, form)
	if err != nil {
		return Profile{}, err
	}
	return c.profileFromToken(ctx, provider, discovery, token, nonce)
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
	if provider.OIDC.MCPResource != "" {
		form.Set("resource", provider.OIDC.MCPResource)
	}
	if len(provider.OIDC.Scopes) > 0 {
		form.Set("scope", strings.Join(provider.OIDC.Scopes, " "))
	}
	token, err := c.token(ctx, discovery.TokenEndpoint, form)
	if err != nil {
		return Profile{}, err
	}
	if provider.Type == ProviderBroker && (!strings.EqualFold(token.TokenType, "Bearer") || token.ExpiresIn <= 0) {
		return Profile{}, errors.New("broker refresh must return a Bearer token with positive expires_in")
	}
	if provider.Type == ProviderBroker && (token.RefreshToken == "" || token.RefreshToken == session.RefreshToken) {
		return Profile{}, errors.New("broker did not rotate the refresh token")
	}
	if token.RefreshToken == "" {
		token.RefreshToken = session.RefreshToken
	}
	if token.IDToken == "" {
		if token.AccessToken == "" {
			return Profile{}, errors.New("OIDC refresh response must include access_token")
		}
		if provider.Type == ProviderBroker {
			if err := c.verifyBrokerAccessToken(ctx, provider, discovery, token, profile.Subject); err != nil {
				return Profile{}, err
			}
		}
		profile.OIDC = &OIDCSession{AccessToken: token.AccessToken, RefreshToken: token.RefreshToken, IDToken: session.IDToken, TokenType: token.TokenType, ExpiresAt: c.now().Add(time.Duration(token.ExpiresIn) * time.Second)}
		return profile, nil
	}
	updated, err := c.profileFromToken(ctx, provider, discovery, token, "")
	if err != nil {
		return Profile{}, err
	}
	if updated.Issuer != profile.Issuer || updated.Subject != profile.Subject {
		return Profile{}, errors.New("refreshed ID token identity does not match the existing profile")
	}
	return updated, nil
}

func (c *OIDCClient) profileFromToken(ctx context.Context, provider Provider, discovery OIDCDiscovery, token tokenResponse, nonce string) (Profile, error) {
	if token.AccessToken == "" || token.IDToken == "" {
		return Profile{}, errors.New("OIDC token response must include access_token and id_token")
	}
	if provider.Type == ProviderBroker && (!strings.EqualFold(token.TokenType, "Bearer") || token.ExpiresIn <= 0) {
		return Profile{}, errors.New("broker token response must contain a Bearer token with positive expires_in")
	}
	idTokenKind := "ID token"
	if provider.Type == ProviderBroker {
		idTokenKind = "Broker ID token"
	}
	claims, err := c.verifySignedToken(ctx, discovery, []string{provider.OIDC.ClientID}, token.IDToken, nonce, idTokenKind, true)
	if err != nil {
		return Profile{}, err
	}
	issuer, _ := claims["iss"].(string)
	subject, _ := claims["sub"].(string)
	if subject == "" {
		return Profile{}, errors.New("verified ID token has no subject")
	}
	if provider.Type == ProviderBroker {
		if !audienceOnly(claims["aud"], provider.OIDC.ClientID) {
			return Profile{}, errors.New("broker ID token has an unexpected audience")
		}
		if _, ok := claims["iat"].(float64); !ok {
			return Profile{}, errors.New("broker ID token has no numeric issued-at time")
		}
		if _, ok := claims["exp"].(float64); !ok {
			return Profile{}, errors.New("broker ID token has no numeric expiration")
		}
		if err := c.verifyBrokerAccessToken(ctx, provider, discovery, token, subject); err != nil {
			return Profile{}, err
		}
	}
	profile := Profile{Provider: provider.Name, ProviderRevision: provider.Revision, Issuer: issuer, Subject: subject,
		OIDC: &OIDCSession{AccessToken: token.AccessToken, RefreshToken: token.RefreshToken, IDToken: token.IDToken, TokenType: token.TokenType, ExpiresAt: c.now().Add(time.Duration(token.ExpiresIn) * time.Second)}}
	if err := mapProfileClaims(&profile, provider.OIDC, claims); err != nil {
		return Profile{}, err
	}
	return profile, nil
}

func (c *OIDCClient) verifyBrokerAccessToken(ctx context.Context, provider Provider, discovery OIDCDiscovery, token tokenResponse, subject string) error {
	if token.AccessToken == "" || provider.OIDC == nil || provider.OIDC.MCPAudience == "" {
		return errors.New("broker access token or audience is missing")
	}
	claims, err := c.verifySignedToken(ctx, discovery, []string{provider.OIDC.MCPAudience}, token.AccessToken, "", "Broker access token", true)
	if err != nil {
		return fmt.Errorf("verify broker access token: %w", err)
	}
	if claims["token_use"] != "access" || claims["client_id"] != provider.OIDC.ClientID || claims["sub"] != subject {
		return errors.New("broker access token use, client, or subject does not match login")
	}
	if _, ok := claims["iat"].(float64); !ok {
		return errors.New("broker access token has no numeric issued-at time")
	}
	if _, ok := claims["exp"].(float64); !ok {
		return errors.New("broker access token has no numeric expiration")
	}
	if jti, ok := claims["jti"].(string); !ok || jti == "" {
		return errors.New("broker access token has no token identifier")
	}
	if resource := provider.OIDC.MCPResource; resource != "" && !audienceContainsAny(claims["aud"], []string{resource}) {
		return errors.New("broker access token does not target the configured MCP resource")
	}
	return nil
}

func audienceOnly(value any, want string) bool {
	switch audience := value.(type) {
	case string:
		return audience == want
	case []any:
		return len(audience) == 1 && audience[0] == want
	}
	return false
}

func mapProfileClaims(profile *Profile, config *OIDCConfig, claims map[string]any) error {
	username, err := stringClaim(claims, config.UsernameClaim, true)
	if err != nil {
		return err
	}
	organization, err := stringClaim(claims, config.OrganizationClaim, false)
	if err != nil {
		return err
	}
	teams, err := stringSliceClaim(claims, config.TeamsClaim)
	if err != nil {
		return err
	}
	profile.Username, profile.Organization, profile.Teams = username, organization, teams
	return nil
}

func (c *OIDCClient) token(ctx context.Context, endpoint string, form url.Values) (tokenResponse, error) {
	var token tokenResponse
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, endpoint, strings.NewReader(form.Encode()))
	if err != nil {
		return token, err
	}
	req.Header.Set("Content-Type", "application/x-www-form-urlencoded")
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

func (c *OIDCClient) verifySignedToken(ctx context.Context, discovery OIDCDiscovery, acceptedAudiences []string, raw, nonce, kind string, requireAudience bool) (map[string]any, error) {
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
	if (kind == "Broker ID token" || kind == "Broker access token") && header.Alg != "EdDSA" {
		return nil, fmt.Errorf("%s must use the Broker EdDSA signing algorithm", kind)
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
	if iss, _ := claims["iss"].(string); iss != discovery.Issuer {
		return nil, fmt.Errorf("%s issuer does not match provider", kind)
	}
	_, hasAudience := claims["aud"]
	if (requireAudience || hasAudience && len(acceptedAudiences) > 0) && !audienceContainsAny(claims["aud"], acceptedAudiences) {
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
	const maxJSON = 2 << 20
	data, err := io.ReadAll(io.LimitReader(r, maxJSON+1))
	if err != nil {
		return err
	}
	if len(data) > maxJSON {
		return errors.New("JSON response exceeds 2 MiB")
	}
	return json.Unmarshal(data, out)
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

// audienceContainsAny reports whether the token's aud claim names any accepted audience.
// A token may legitimately carry several audiences: RFC 8707 adds the requested resource
// alongside whatever deployment-wide audience the authorization server mints.
func audienceContainsAny(v any, accepted []string) bool {
	match := func(candidate string) bool {
		for _, want := range accepted {
			if candidate == want {
				return true
			}
		}
		return false
	}
	switch a := v.(type) {
	case string:
		return match(a)
	case []any:
		for _, value := range a {
			if s, ok := value.(string); ok && match(s) {
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
	values, err := selectClaimValues(claims, name)
	if err != nil {
		return "", err
	}
	if len(values) == 0 {
		if required {
			return "", fmt.Errorf("verified claims have no %q claim", name)
		}
		return "", nil
	}
	if len(values) != 1 {
		return "", fmt.Errorf("claim selector %q must select exactly one value, got %d", name, len(values))
	}
	s, ok := values[0].(string)
	if !ok || strings.TrimSpace(s) == "" {
		return "", fmt.Errorf("verified claim %q must be a non-empty string", name)
	}
	return s, nil
}
func stringSliceClaim(claims map[string]any, name string) ([]string, error) {
	if name == "" {
		return nil, nil
	}
	values, err := selectClaimValues(claims, name)
	if err != nil {
		return nil, err
	}
	if len(values) == 0 {
		return nil, nil
	}
	if len(values) == 1 {
		if array, ok := values[0].([]any); ok {
			values = array
		}
	}
	out := make([]string, 0, len(values))
	for _, value := range values {
		s, ok := value.(string)
		if !ok {
			return nil, fmt.Errorf("verified claim %q must select strings or one string array", name)
		}
		if s != "" {
			out = append(out, s)
		}
	}
	return out, nil
}

// selectClaimValues treats non-JSONPath selectors as literal top-level keys.
func selectClaimValues(claims map[string]any, name string) ([]any, error) {
	if !strings.HasPrefix(name, "$") {
		if value, ok := claims[name]; ok {
			return []any{value}, nil
		}
		return nil, nil
	}
	path, err := jsonpath.Parse(name)
	if err != nil {
		return nil, fmt.Errorf("invalid claim JSONPath %q: %w", name, err)
	}
	return path.Select(claims), nil
}
