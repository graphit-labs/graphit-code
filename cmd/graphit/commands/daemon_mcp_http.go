package commands

import (
	"context"
	"fmt"
	"net/http"
	"strings"
	"time"

	mcpauth "github.com/modelcontextprotocol/go-sdk/auth"
	"github.com/modelcontextprotocol/go-sdk/oauthex"
	"github.com/rs/cors"

	"github.com/graphit-labs/graphit-code/internal/agentpolicy"
	"github.com/graphit-labs/graphit-code/internal/auth"
)

// Headers the Streamable HTTP transport exchanges. The SDK declares them unexported in
// mcp/streamable_headers.go, so a browser client can only reach this endpoint if the CORS
// policy names them here. TestMCPTransportHeaderNamesMatchSDK guards the copies.
const (
	mcpSessionIDHeader       = "Mcp-Session-Id"
	mcpProtocolVersionHeader = "Mcp-Protocol-Version"
)

// staticCredentialLifetime is the session window reported for credentials that carry no expiry:
// the daemon runtime key and a local profile's MCP key. The SDK's bearer middleware refuses a
// TokenInfo without an expiration, and these are revoked by restarting the daemon or switching
// profile rather than by elapsing.
const staticCredentialLifetime = time.Minute

// No declared origin means no middleware at all: a deployment becomes reachable from a browser
// only when its operator says so. Credentials stay off because MCP authenticates with the
// Authorization header rather than cookies, which also makes the invalid
// wildcard-plus-credentials combination unrepresentable here.
func newMCPCORS(allowedOrigins []string) func(http.Handler) http.Handler {
	if len(allowedOrigins) == 0 {
		return func(next http.Handler) http.Handler { return next }
	}
	policy := cors.New(cors.Options{
		AllowedOrigins: allowedOrigins,
		AllowedMethods: []string{http.MethodGet, http.MethodPost, http.MethodDelete, http.MethodOptions},
		AllowedHeaders: []string{
			"Authorization",
			"Content-Type",
			"Last-Event-ID",
			mcpSessionIDHeader,
			mcpProtocolVersionHeader,
			agentpolicy.ProfileHeader,
		},
		// Without WWW-Authenticate exposed, a browser client receives the 401 but cannot
		// read the challenge, so it never learns where to authenticate and the whole
		// discovery path is dead for it.
		ExposedHeaders:   []string{"WWW-Authenticate", mcpSessionIDHeader},
		AllowCredentials: false,
	})
	return policy.Handler
}

// activeMCPProvider reads the active provider without refreshing its session. Advertising
// metadata is a read: it must never mutate the daemon's own login, and it must not fail
// merely because that login expired.
func activeMCPProvider() (auth.Provider, bool) {
	store, err := auth.Open()
	if err != nil {
		return auth.Provider{}, false
	}
	snapshot, err := store.Active()
	if err != nil {
		return auth.Provider{}, false
	}
	return snapshot.Provider, true
}

func mcpAcceptedAudiences(resolver *auth.ProtectedResourceResolver, r *http.Request) ([]string, bool) {
	if resolver == nil {
		return nil, false
	}
	provider, ok := activeMCPProvider()
	if !ok {
		return nil, false
	}
	return resolver.AcceptedAudiences(r.Context(), provider, r.Host)
}

func mcpProtectedResource(resolver *auth.ProtectedResourceResolver, r *http.Request) (auth.ProtectedResource, bool) {
	if resolver == nil {
		return auth.ProtectedResource{}, false
	}
	provider, ok := activeMCPProvider()
	if !ok {
		return auth.ProtectedResource{}, false
	}
	return resolver.Resolve(r.Context(), provider, r.Host)
}

type mcpBearerHandler struct {
	next       http.Handler
	runtimeKey string
	resolver   *auth.ProtectedResourceResolver
	verifier   daemonAccessTokenVerifier
}

// ServeHTTP builds the SDK's bearer middleware per request, because the challenge it emits
// depends on request state: which resource this daemon is, and whether a resource is resolvable
// at all. With nothing resolvable the challenge carries no resource_metadata, and bearer
// authentication still works for callers that already hold a credential.
func (h *mcpBearerHandler) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	// The SDK hands the verifier's TokenInfo to the next handler, but the identity this
	// daemon derives lives in a context. Capturing it here is safe because the closure and
	// the middleware are both scoped to this single request.
	var authorized context.Context
	var authorizedProfile string

	// Production leaves Verifier unset; only tests inject one. Resolving the default here
	// rather than dereferencing a nil interface is what keeps a real broker token from
	// panicking the listener.
	tokenVerifier := h.verifier
	if tokenVerifier == nil {
		tokenVerifier = auth.NewProviderAccessTokenVerifier()
	}

	// The audiences this endpoint answers to, resolved from whatever the active provider
	// publishes about itself rather than from anything configured per provider type.
	acceptedAudiences, _ := mcpAcceptedAudiences(h.resolver, r)

	verifier := func(ctx context.Context, token string, _ *http.Request) (*mcpauth.TokenInfo, error) {
		requestContext := ctx
		allowed := false
		if profile, ok := agentpolicy.VerifyCapabilityToken(h.runtimeKey, token); ok {
			authorizedProfile = profile
			allowed = true
		} else {
			authorizedProfile = ""
			requestContext, allowed = daemonBearerContextWithVerifier(ctx, token, h.runtimeKey, tokenVerifier, acceptedAudiences)
		}
		if !allowed {
			return nil, mcpauth.ErrInvalidToken
		}
		authorized = requestContext
		info := &mcpauth.TokenInfo{Expiration: time.Now().Add(staticCredentialLifetime)}
		if identity, ok := auth.RequestIdentity(requestContext); ok {
			info.UserID = identity.Username
			if !identity.ExpiresAt.IsZero() {
				info.Expiration = identity.ExpiresAt
			}
		}
		return info, nil
	}

	opts := &mcpauth.RequireBearerTokenOptions{}
	if resource, ok := mcpProtectedResource(h.resolver, r); ok {
		opts.ResourceMetadataURL = resource.MetadataURL()
	}
	// opts.Scopes stays unset: a token's right to be here comes from its audience, so demanding
	// a scope on top would only narrow which authorization servers can serve the endpoint.
	authenticated := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if authorized != nil {
			r = r.WithContext(authorized)
		}
		r = r.Clone(r.Context())
		r.Header.Del(agentpolicy.ProfileHeader)
		if authorizedProfile != "" {
			r.Header.Set(agentpolicy.ProfileHeader, authorizedProfile)
		}
		h.next.ServeHTTP(&mcpPermissionChallengeWriter{ResponseWriter: w, resourceMetadataURL: opts.ResourceMetadataURL}, r)
	})
	mcpauth.RequireBearerToken(verifier, opts)(authenticated).ServeHTTP(&mcpBearerChallengeWriter{
		ResponseWriter: w, authorization: r.Header.Get("Authorization"),
	}, r)
}

// The SDK omits WWW-Authenticate when resource metadata cannot be resolved and
// does not distinguish an invalid bearer from an absent one in its challenge.
// RFC 6750 still requires a Bearer challenge on 401, with invalid_token for an
// attempted bearer that verification rejected.
type mcpBearerChallengeWriter struct {
	http.ResponseWriter
	authorization string
}

func (w *mcpBearerChallengeWriter) Unwrap() http.ResponseWriter { return w.ResponseWriter }

func (w *mcpBearerChallengeWriter) WriteHeader(status int) {
	if status == http.StatusUnauthorized {
		challenge := ""
		for _, candidate := range w.Header().Values("WWW-Authenticate") {
			if candidate == "Bearer" || strings.HasPrefix(candidate, "Bearer ") {
				challenge = candidate
				break
			}
		}
		if challenge == "" {
			challenge = "Bearer"
		}
		fields := strings.Fields(w.authorization)
		if len(fields) == 2 && strings.EqualFold(fields[0], "Bearer") && fields[1] != "" && !strings.Contains(challenge, "error=") {
			if challenge == "Bearer" {
				challenge += ` error="invalid_token"`
			} else {
				challenge += `, error="invalid_token"`
			}
		}
		w.Header().Set("WWW-Authenticate", challenge)
	}
	w.ResponseWriter.WriteHeader(status)
}

// A downstream operation can identify its required scope after the bearer has been
// verified. Keep that 403 and its scope challenge, and make the same discovery URL
// available as on the initial 401.
type mcpPermissionChallengeWriter struct {
	http.ResponseWriter
	resourceMetadataURL string
}

func (w *mcpPermissionChallengeWriter) Unwrap() http.ResponseWriter { return w.ResponseWriter }

func (w *mcpPermissionChallengeWriter) WriteHeader(status int) {
	if status == http.StatusForbidden && w.resourceMetadataURL != "" {
		for _, challenge := range w.Header().Values("WWW-Authenticate") {
			if strings.HasPrefix(challenge, "Bearer ") && strings.Contains(challenge, `error="insufficient_scope"`) && !strings.Contains(challenge, "resource_metadata=") {
				w.Header().Set("WWW-Authenticate", challenge+", "+fmt.Sprintf("resource_metadata=%q", w.resourceMetadataURL))
				break
			}
		}
	}
	w.ResponseWriter.WriteHeader(status)
}

// mcpMetadataHandler serves the protected resource metadata document (RFC 9728).
//
// It is public by design: the SDK handler allows any origin because the document exists so
// that an unauthenticated client can discover where to authenticate. When no resource is
// resolvable the daemon announces no authorization server rather than guessing one, which
// is the state a freshly started container is in before anyone logs in.
func mcpMetadataHandler(resolver *auth.ProtectedResourceResolver) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		resource, ok := mcpProtectedResource(resolver, r)
		if !ok || (r.URL.Path != resource.MetadataPath() && r.URL.Path != "/.well-known/oauth-protected-resource") {
			http.NotFound(w, r)
			return
		}
		mcpauth.ProtectedResourceMetadataHandler(&oauthex.ProtectedResourceMetadata{
			Resource:               resource.Resource,
			AuthorizationServers:   resource.AuthorizationServers,
			ScopesSupported:        resource.ScopesSupported,
			BearerMethodsSupported: []string{"header"},
		}).ServeHTTP(w, r)
	})
}
