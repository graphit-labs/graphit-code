package auth

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strings"
)

const brokerOIDCAuthenticationType = "openid_connect"

// BrokerOIDCProvider resolves the Broker-owned client registration into the same
// OIDC provider shape used for every other interactive login. The returned
// provider is transient: deployment-owned OIDC details remain discoverable and
// are not copied into Graphit Code's persisted provider configuration.
func BrokerOIDCProvider(ctx context.Context, provider Provider, client *http.Client) (Provider, error) {
	if provider.Type != ProviderBroker || provider.Broker == nil {
		return Provider{}, errors.New("provider is not a broker authentication provider")
	}
	discovery, err := DiscoverBroker(ctx, provider, client)
	if err != nil {
		return Provider{}, err
	}
	authentication := discovery.Authentication
	if authentication.Type != brokerOIDCAuthenticationType || strings.TrimSpace(authentication.Issuer) == "" || strings.TrimSpace(authentication.ClientID) == "" {
		return Provider{}, errors.New("broker does not advertise OpenID Connect authentication")
	}
	if err := sameBrokerOrigin(provider.Broker.Endpoint, authentication.Issuer); err != nil {
		return Provider{}, fmt.Errorf("broker OIDC issuer: %w", err)
	}
	if strings.TrimRight(authentication.Issuer, "/") != strings.TrimRight(discovery.Issuer, "/") {
		return Provider{}, errors.New("broker OIDC issuer does not match broker discovery")
	}
	if authentication.RedirectURIPath == "" || !strings.HasPrefix(authentication.RedirectURIPath, "/") || strings.ContainsAny(authentication.RedirectURIPath, "?#") {
		return Provider{}, errors.New("broker advertises an invalid OIDC redirect path")
	}
	if !contains(authentication.Scopes, "openid") || !contains(authentication.Scopes, "graphit.use") {
		return Provider{}, errors.New("broker OIDC scopes must include openid and graphit.use")
	}
	oidcClient := &OIDCClient{HTTP: client}
	standard, err := oidcClient.Discovery(ctx, authentication.Issuer)
	if err != nil {
		return Provider{}, err
	}
	if !contains(standard.GrantTypesSupported, "authorization_code") || !contains(standard.GrantTypesSupported, "refresh_token") ||
		!contains(standard.CodeChallengeMethodsSupported, "S256") || !contains(standard.TokenEndpointAuthMethodsSupported, "none") ||
		!contains(standard.IDTokenSigningAlgorithmsSupported, "EdDSA") {
		return Provider{}, errors.New("broker OIDC metadata lacks Authorization Code, refresh, PKCE S256, public-client, or EdDSA support")
	}
	for _, endpoint := range []string{standard.AuthorizationEndpoint, standard.TokenEndpoint, standard.JWKSURI, standard.UserinfoEndpoint} {
		if err := sameBrokerOrigin(provider.Broker.Endpoint, endpoint); err != nil {
			return Provider{}, fmt.Errorf("broker OIDC endpoint: %w", err)
		}
	}
	resolved := provider
	resolved.OIDC = &OIDCConfig{
		Issuer:            strings.TrimRight(authentication.Issuer, "/"),
		ClientID:          authentication.ClientID,
		TokenAuthMethod:   "none",
		Scopes:            append([]string(nil), authentication.Scopes...),
		RedirectURIPath:   authentication.RedirectURIPath,
		UsernameClaim:     "preferred_username",
		OrganizationClaim: "organization",
		TeamsClaim:        "groups",
	}
	return resolved, nil
}

type brokerUserinfo struct {
	Subject      string   `json:"sub"`
	Username     string   `json:"preferred_username"`
	Organization string   `json:"organization,omitempty"`
	Teams        []string `json:"groups,omitempty"`
}

func verifyBrokerAccessToken(ctx context.Context, client *OIDCClient, provider Provider, raw string) (VerifiedIdentity, error) {
	if strings.TrimSpace(raw) == "" {
		return VerifiedIdentity{}, errors.New("broker access token is required")
	}
	if client == nil {
		client = NewOIDCClient()
	}
	resolved, err := BrokerOIDCProvider(ctx, provider, client.client())
	if err != nil {
		return VerifiedIdentity{}, err
	}
	discovery, err := client.Discovery(ctx, resolved.OIDC.Issuer)
	if err != nil {
		return VerifiedIdentity{}, err
	}
	if discovery.UserinfoEndpoint == "" {
		return VerifiedIdentity{}, errors.New("broker OIDC discovery omitted userinfo_endpoint")
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, discovery.UserinfoEndpoint, nil)
	if err != nil {
		return VerifiedIdentity{}, err
	}
	req.Header.Set("Authorization", "Bearer "+raw)
	req.Header.Set("Accept", "application/json")
	resp, err := client.client().Do(req)
	if err != nil {
		return VerifiedIdentity{}, fmt.Errorf("verify broker access token: %w", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		_, _ = io.Copy(io.Discard, io.LimitReader(resp.Body, 4096))
		return VerifiedIdentity{}, fmt.Errorf("broker rejected access token with HTTP %d", resp.StatusCode)
	}
	var identity brokerUserinfo
	if err := json.NewDecoder(io.LimitReader(resp.Body, 1<<20)).Decode(&identity); err != nil {
		return VerifiedIdentity{}, fmt.Errorf("decode broker OIDC userinfo: %w", err)
	}
	if strings.TrimSpace(identity.Subject) == "" || strings.TrimSpace(identity.Username) == "" {
		return VerifiedIdentity{}, errors.New("broker OIDC userinfo is incomplete")
	}
	return VerifiedIdentity{Issuer: resolved.OIDC.Issuer, Subject: identity.Subject, Username: identity.Username,
		Organization: identity.Organization, Teams: append([]string(nil), identity.Teams...)}, nil
}

type ProviderAccessTokenVerifier struct{ OIDC *OIDCClient }

func NewProviderAccessTokenVerifier() *ProviderAccessTokenVerifier {
	return &ProviderAccessTokenVerifier{OIDC: NewOIDCClient()}
}

func (v *ProviderAccessTokenVerifier) VerifyAccessToken(ctx context.Context, provider Provider, raw, audience string) (VerifiedIdentity, error) {
	client := v.OIDC
	if client == nil {
		client = NewOIDCClient()
	}
	switch provider.Type {
	case ProviderOIDC:
		return client.VerifyAccessToken(ctx, provider, raw, audience)
	case ProviderBroker:
		return verifyBrokerAccessToken(ctx, client, provider, raw)
	default:
		return VerifiedIdentity{}, errors.New("provider does not verify remote access tokens")
	}
}

func sameBrokerOrigin(configured, advertised string) error {
	base, err := url.Parse(strings.TrimSpace(configured))
	if err != nil {
		return errors.New("configured broker endpoint is invalid")
	}
	target, err := url.Parse(strings.TrimSpace(advertised))
	if err != nil || target.Scheme == "" || target.Host == "" || target.User != nil || target.Fragment != "" {
		return errors.New("advertised URL is invalid")
	}
	if !strings.EqualFold(base.Scheme, target.Scheme) || !strings.EqualFold(base.Host, target.Host) {
		return errors.New("advertised URL is outside the configured broker origin")
	}
	return nil
}
