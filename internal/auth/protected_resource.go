package auth

import (
	"context"
	"errors"
	"net/http"
	"net/url"
	"strings"
	"sync"
	"time"
)

var errBrokerNoOIDC = errors.New("broker does not advertise OpenID Connect authentication")

// wellKnownProtectedResource is the RFC 9728 well-known path prefix. A resource with a path
// component is published under that path appended to the prefix, so one host can serve
// metadata for several protected resources without collision.
const wellKnownProtectedResource = "/.well-known/oauth-protected-resource"

// defaultProtectedResourceTTL bounds how long a broker advertisement is reused. Discovery is
// an HTTP call on an unauthenticated, frequently hit path, so resolving it per request would
// put the authorization server in the hot path of every MCP call.
const defaultProtectedResourceTTL = 5 * time.Minute

// ProtectedResource is the OAuth 2.0 protected resource metadata (RFC 9728) that the MCP
// listener publishes so an external client can find the authorization server on its own.
type ProtectedResource struct {
	Resource             string
	AuthorizationServers []string
	ScopesSupported      []string
}

// MetadataPath is where this resource's metadata document belongs, per RFC 9728 section 3.
func (p ProtectedResource) MetadataPath() string {
	parsed, err := url.Parse(p.Resource)
	if err != nil {
		return wellKnownProtectedResource
	}
	path := strings.Trim(parsed.Path, "/")
	if path == "" {
		return wellKnownProtectedResource
	}
	return wellKnownProtectedResource + "/" + path
}

// MetadataURL is the absolute location a client is told to fetch in the WWW-Authenticate
// challenge. It is derived from the resource itself rather than from how this process was
// reached, so a proxied deployment still advertises its canonical address.
func (p ProtectedResource) MetadataURL() string {
	parsed, err := url.Parse(p.Resource)
	if err != nil || parsed.Scheme == "" || parsed.Host == "" {
		return ""
	}
	return parsed.Scheme + "://" + parsed.Host + p.MetadataPath()
}

// AcceptedAudiences is the set of `aud` values a token may carry for this resource.
//
// RFC 8707 makes the resource itself an audience, and an authorization server may also mint a
// deployment-wide audience. Both are legitimate for the same endpoint, so both are accepted
// and nothing else is.
func (p ProtectedResource) AcceptedAudiences(extra ...string) []string {
	accepted := make([]string, 0, len(extra)+1)
	add := func(value string) {
		value = strings.TrimSpace(value)
		if value == "" {
			return
		}
		for _, existing := range accepted {
			if existing == value {
				return
			}
		}
		accepted = append(accepted, value)
	}
	add(p.Resource)
	for _, value := range extra {
		add(value)
	}
	return accepted
}

// advertisement is what a provider publishes about itself, however it was learned.
type advertisement struct {
	issuer    string
	scopes    []string
	resources []string
	// audience is the deployment-wide audience the authorization server mints in addition to
	// any requested resource. Empty when the provider does not define one.
	audience string
	// matchHost says whether a resource only applies when the request arrived at its host.
	// A broker publishes one list covering every deployment that uses it, so a daemon proves
	// which entry is its own. A direct OIDC provider is configured on this daemon alone, so
	// its resource is already specific and needs no such proof.
	matchHost bool
}

// ProtectedResourceResolver answers what the MCP listener may advertise for the active
// provider. Broker advertisements are cached and survive a brief outage: losing discovery
// must not turn the listener into a server that silently stops telling clients where to
// authenticate.
type ProtectedResourceResolver struct {
	HTTP *http.Client
	TTL  time.Duration
	Now  func() time.Time

	mu        sync.Mutex
	cached    advertisement
	cachedKey string
	cachedAt  time.Time
	hasCached bool
}

func NewProtectedResourceResolver() *ProtectedResourceResolver {
	return &ProtectedResourceResolver{TTL: defaultProtectedResourceTTL}
}

// Resolve reports the metadata to advertise for this provider when reached at requestHost,
// and whether anything may be advertised at all.
//
// It fails closed: a provider that publishes no MCP resource, and a host that is absent from
// a broker's advertised list, both yield ok=false, which the caller turns into "no
// authorization server announced" rather than into a guess.
func (r *ProtectedResourceResolver) Resolve(ctx context.Context, provider Provider, requestHost string) (ProtectedResource, bool) {
	advert, ok := r.advertisementFor(ctx, provider)
	if !ok {
		return ProtectedResource{}, false
	}
	resource, ok := selectAdvertisedResource(advert.resources, requestHost, advert.matchHost)
	if !ok {
		return ProtectedResource{}, false
	}
	return ProtectedResource{
		Resource:             resource,
		AuthorizationServers: []string{advert.issuer},
		ScopesSupported:      append([]string(nil), advert.scopes...),
	}, true
}

// AcceptedAudiences reports every `aud` value this provider's tokens may legitimately carry
// for the MCP endpoint, or false when the provider publishes no resource at all.
func (r *ProtectedResourceResolver) AcceptedAudiences(ctx context.Context, provider Provider, requestHost string) ([]string, bool) {
	advert, ok := r.advertisementFor(ctx, provider)
	if !ok {
		return nil, false
	}
	resource, ok := selectAdvertisedResource(advert.resources, requestHost, advert.matchHost)
	if !ok {
		return nil, false
	}
	return ProtectedResource{Resource: resource}.AcceptedAudiences(advert.audience), true
}

func (r *ProtectedResourceResolver) advertisementFor(ctx context.Context, provider Provider) (advertisement, bool) {
	switch provider.Type {
	case ProviderBroker:
		if provider.Broker == nil {
			return advertisement{}, false
		}
		return r.brokerAdvertisement(ctx, provider)
	case ProviderOIDC:
		return directOIDCAdvertisement(provider)
	default:
		// A local provider has no authorization server to announce.
		return advertisement{}, false
	}
}

// directOIDCAdvertisement builds the advertisement from the provider's own OIDC configuration.
//
// For this provider type the operator configured the issuer, the scopes and the MCP resource
// on this daemon directly, so that configuration is the authority, exactly as broker discovery
// is the authority for a broker-managed provider.
func directOIDCAdvertisement(provider Provider) (advertisement, bool) {
	if provider.OIDC == nil {
		return advertisement{}, false
	}
	issuer := strings.TrimRight(strings.TrimSpace(provider.OIDC.Issuer), "/")
	resource := strings.TrimSpace(provider.OIDC.MCPResource)
	if issuer == "" || resource == "" {
		// Without a canonical resource there is nothing to publish metadata about, and
		// advertising the issuer alone would tell a client nothing it could act on.
		return advertisement{}, false
	}
	return advertisement{
		issuer:    issuer,
		scopes:    append([]string(nil), provider.OIDC.Scopes...),
		resources: []string{resource},
		audience:  strings.TrimSpace(provider.OIDC.MCPAudience),
		matchHost: false,
	}, true
}

func (r *ProtectedResourceResolver) brokerAdvertisement(ctx context.Context, provider Provider) (advertisement, bool) {
	key := provider.Name + "\x00" + strings.TrimRight(strings.TrimSpace(provider.Broker.Endpoint), "/")

	r.mu.Lock()
	cached, cachedKey, cachedAt, hasCached := r.cached, r.cachedKey, r.cachedAt, r.hasCached
	r.mu.Unlock()

	if hasCached && cachedKey == key && r.now().Sub(cachedAt) < r.ttl() {
		return cached, true
	}

	advert, err := fetchBrokerAdvertisement(ctx, provider, r.HTTP)
	if err != nil {
		// A transient outage keeps the last advertisement this provider produced. Serving a
		// stale but authentic value beats telling clients there is no authorization server.
		// A different provider gets nothing, because the cached value would then describe a
		// deployment the caller is not talking to.
		if hasCached && cachedKey == key {
			return cached, true
		}
		return advertisement{}, false
	}

	r.mu.Lock()
	r.cached, r.cachedKey, r.cachedAt, r.hasCached = advert, key, r.now(), true
	r.mu.Unlock()
	return advert, true
}

func fetchBrokerAdvertisement(ctx context.Context, provider Provider, client *http.Client) (advertisement, error) {
	discovery, err := DiscoverBroker(ctx, provider, client)
	if err != nil {
		return advertisement{}, err
	}
	authentication := discovery.Authentication
	if authentication.Type != brokerOIDCAuthenticationType || strings.TrimSpace(authentication.Issuer) == "" {
		return advertisement{}, errBrokerNoOIDC
	}
	// The same origin check login performs. Without it a tampered discovery document could
	// point every MCP client at an authorization server of the attacker's choosing.
	if err := sameBrokerOrigin(provider.Broker.Endpoint, authentication.Issuer); err != nil {
		return advertisement{}, err
	}
	return advertisement{
		issuer:    strings.TrimRight(authentication.Issuer, "/"),
		scopes:    append([]string(nil), authentication.Scopes...),
		resources: append([]string(nil), authentication.MCPResources...),
		audience:  strings.TrimSpace(authentication.AccessTokenAudience),
		matchHost: true,
	}, nil
}

// selectAdvertisedResource picks which advertised resource this daemon is.
//
// When matchHost is set the advertised list is shared across deployments, so requestHost only
// chooses among entries the authorization server already vouched for. Consulting it is safe
// even though a caller controls the Host header: an unlisted host selects nothing, and no
// value is ever derived from the request itself. A lone entry is no exception, because one
// entry is not evidence that it describes this deployment rather than a sibling.
func selectAdvertisedResource(resources []string, requestHost string, matchHost bool) (string, bool) {
	cleaned := make([]string, 0, len(resources))
	for _, resource := range resources {
		if trimmed := strings.TrimSpace(resource); trimmed != "" {
			cleaned = append(cleaned, trimmed)
		}
	}
	if len(cleaned) == 0 {
		return "", false
	}
	if !matchHost {
		if len(cleaned) != 1 {
			// Configuration scoped to this daemon must name exactly one canonical resource;
			// several would leave the identity ambiguous.
			return "", false
		}
		return cleaned[0], true
	}
	host := requestHostname(requestHost)
	if host == "" {
		return "", false
	}
	for _, resource := range cleaned {
		parsed, err := url.Parse(resource)
		if err != nil {
			continue
		}
		if strings.EqualFold(parsed.Hostname(), host) {
			return resource, true
		}
	}
	return "", false
}

func requestHostname(requestHost string) string {
	host := strings.TrimSpace(requestHost)
	if host == "" {
		return ""
	}
	parsed, err := url.Parse("//" + host)
	if err != nil {
		return ""
	}
	return parsed.Hostname()
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
