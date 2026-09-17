package auth

import (
	"context"
	"errors"
	"fmt"
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
	// Only openid is required, because OIDC Core requires it of any OpenID Provider. Graphit
	// asks for no product-specific scope: a token's right to reach this endpoint comes from
	// its audience, not from a scope name a given authorization server may not offer.
	if !contains(authentication.Scopes, "openid") {
		return Provider{}, errors.New("broker OIDC scopes must include openid")
	}
	if strings.TrimSpace(authentication.AccessTokenAudience) == "" || !contains(authentication.Audiences, authentication.AccessTokenAudience) {
		return Provider{}, errors.New("broker OIDC access token audience is missing or inconsistent with advertised audiences")
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
	if standard.UserinfoEndpoint == "" {
		return Provider{}, errors.New("broker OIDC metadata has no userinfo endpoint")
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
		MCPAudience:       authentication.AccessTokenAudience,
	}
	return resolved, nil
}

type ProviderAccessTokenVerifier struct{ OIDC *OIDCClient }

func NewProviderAccessTokenVerifier() *ProviderAccessTokenVerifier {
	return &ProviderAccessTokenVerifier{OIDC: NewOIDCClient()}
}

func (v *ProviderAccessTokenVerifier) VerifyAccessToken(ctx context.Context, provider Provider, raw string, audiences []string) (VerifiedIdentity, error) {
	client := v.OIDC
	if client == nil {
		client = NewOIDCClient()
	}
	switch provider.Type {
	case ProviderOIDC:
		return client.VerifyAccessToken(ctx, provider, raw, audiences)
	case ProviderBroker:
		resolved, err := BrokerOIDCProvider(ctx, provider, client.client())
		if err != nil {
			return VerifiedIdentity{}, err
		}
		// The broker's own audience is always acceptable; the caller adds any resource the
		// broker advertises for this endpoint.
		accepted := append([]string{resolved.OIDC.MCPAudience}, audiences...)
		identity, err := client.VerifyAccessToken(ctx, resolved, raw, accepted)
		if err != nil {
			return VerifiedIdentity{}, err
		}
		// The JWT signature and expiry do not reveal broker-side revocation.
		// Ask the issuer to validate this exact bearer before admitting an MCP call.
		standard, err := client.Discovery(ctx, resolved.OIDC.Issuer)
		if err != nil {
			return VerifiedIdentity{}, err
		}
		if standard.UserinfoEndpoint == "" || sameBrokerOrigin(provider.Broker.Endpoint, standard.UserinfoEndpoint) != nil {
			return VerifiedIdentity{}, errors.New("broker userinfo endpoint is unavailable")
		}
		req, err := http.NewRequestWithContext(ctx, http.MethodGet, standard.UserinfoEndpoint, nil)
		if err != nil {
			return VerifiedIdentity{}, err
		}
		req.Header.Set("Authorization", "Bearer "+raw)
		resp, err := client.client().Do(req)
		if err != nil {
			return VerifiedIdentity{}, fmt.Errorf("broker token validation: %w", err)
		}
		defer resp.Body.Close()
		if resp.StatusCode != http.StatusOK {
			return VerifiedIdentity{}, errors.New("broker rejected access token")
		}
		var userinfo struct {
			Subject string `json:"sub"`
		}
		if err := decodeLimited(resp.Body, &userinfo); err != nil || userinfo.Subject != identity.Subject {
			return VerifiedIdentity{}, errors.New("broker userinfo subject does not match access token")
		}
		return identity, nil
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
