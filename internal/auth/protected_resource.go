package auth

import (
	"context"
	"net/http"
	"net/url"
	"strings"
	"sync"
	"time"
)

const wellKnownProtectedResource = "/.well-known/oauth-protected-resource"
const defaultProtectedResourceTTL = 5 * time.Minute

// ProtectedResource is the RFC 9728 document served by a Broker-backed MCP endpoint.
type ProtectedResource struct {
	Resource             string
	AuthorizationServers []string
	ScopesSupported      []string
}

func (p ProtectedResource) MetadataPath() string {
	parsed, err := url.Parse(p.Resource)
	if err != nil || parsed.EscapedPath() == "" || parsed.EscapedPath() == "/" {
		return wellKnownProtectedResource
	}
	return wellKnownProtectedResource + parsed.EscapedPath()
}

func (p ProtectedResource) MetadataURL() string {
	parsed, err := url.Parse(p.Resource)
	if err != nil || parsed.Scheme == "" || parsed.Host == "" {
		return ""
	}
	return parsed.Scheme + "://" + parsed.Host + p.MetadataPath()
}

// An access token must name this exact resource in aud. The Broker API audience
// alone never grants access to the MCP listener.
func (p ProtectedResource) AcceptedAudiences() []string { return []string{p.Resource} }

type brokerMCPAdvertisement struct {
	issuer    string
	scopes    []string
	resources []string
}

type ProtectedResourceResolver struct {
	HTTP *http.Client
	TTL  time.Duration
	Now  func() time.Time

	mu        sync.Mutex
	cached    brokerMCPAdvertisement
	cachedKey string
	cachedAt  time.Time
	hasCached bool
}

func NewProtectedResourceResolver() *ProtectedResourceResolver {
	return &ProtectedResourceResolver{TTL: defaultProtectedResourceTTL}
}

func (r *ProtectedResourceResolver) Resolve(ctx context.Context, provider Provider, requestHost string) (ProtectedResource, bool) {
	if provider.Type != ProviderBroker || provider.Broker == nil || provider.Broker.MCPResource == "" {
		return ProtectedResource{}, false
	}
	configured := provider.Broker.MCPResource
	parsed, err := url.Parse(configured)
	if err != nil || !strings.EqualFold(parsed.Host, strings.TrimSpace(requestHost)) {
		return ProtectedResource{}, false
	}
	advert, ok := r.advertisementFor(ctx, provider)
	if !ok || !contains(advert.resources, configured) || advert.issuer == "" {
		return ProtectedResource{}, false
	}
	return ProtectedResource{Resource: configured, AuthorizationServers: []string{advert.issuer}, ScopesSupported: advert.scopes}, true
}

func (r *ProtectedResourceResolver) AcceptedAudiences(ctx context.Context, provider Provider, requestHost string) ([]string, bool) {
	resource, ok := r.Resolve(ctx, provider, requestHost)
	if !ok {
		return nil, false
	}
	return resource.AcceptedAudiences(), true
}

func (r *ProtectedResourceResolver) advertisementFor(ctx context.Context, provider Provider) (brokerMCPAdvertisement, bool) {
	key := provider.Name + "\x00" + provider.Broker.Endpoint + "\x00" + provider.Broker.MCPResource
	r.mu.Lock()
	cached, cachedKey, cachedAt, hasCached := r.cached, r.cachedKey, r.cachedAt, r.hasCached
	r.mu.Unlock()
	if hasCached && cachedKey == key && r.now().Sub(cachedAt) < r.ttl() {
		return cached, true
	}
	discovery, err := DiscoverBroker(ctx, provider, r.HTTP)
	if err != nil || discovery.Authentication.Type != brokerOIDCAuthenticationType ||
		discovery.Authentication.Issuer == "" || discovery.Authentication.Issuer != discovery.Issuer ||
		sameBrokerOrigin(provider.Broker.Endpoint, discovery.Authentication.Issuer) != nil {
		return brokerMCPAdvertisement{}, false
	}
	advert := brokerMCPAdvertisement{
		issuer:    discovery.Authentication.Issuer,
		resources: append([]string(nil), discovery.Authentication.MCPResources...),
	}
	// The protected resource needs only openid for the Broker OP. In particular,
	// offline_access is a refresh-token request, not an MCP resource requirement.
	if contains(discovery.Authentication.Scopes, "openid") {
		advert.scopes = []string{"openid"}
	}
	r.mu.Lock()
	r.cached, r.cachedKey, r.cachedAt, r.hasCached = advert, key, r.now(), true
	r.mu.Unlock()
	return advert, true
}

func (r *ProtectedResourceResolver) ttl() time.Duration {
	if r.TTL > 0 {
		return r.TTL
	}
	return defaultProtectedResourceTTL
}
func (r *ProtectedResourceResolver) now() time.Time {
	if r.Now != nil {
		return r.Now()
	}
	return time.Now()
}
