package auth

import (
	"context"
	"strings"
)

type requestBrokerBearerKey struct{}
type requestVerifiedIdentityKey struct{}

// An inbound MCP bearer belongs to one request and takes precedence over the
// daemon's own login token for every Broker API call in that request.
func WithBrokerBearer(ctx context.Context, token string) context.Context {
	return context.WithValue(ctx, requestBrokerBearerKey{}, strings.TrimSpace(token))
}
func RequestBrokerBearer(ctx context.Context) string {
	token, _ := ctx.Value(requestBrokerBearerKey{}).(string)
	return strings.TrimSpace(token)
}
func WithRequestIdentity(ctx context.Context, identity VerifiedIdentity) context.Context {
	return context.WithValue(ctx, requestVerifiedIdentityKey{}, identity)
}
func RequestIdentity(ctx context.Context) (VerifiedIdentity, bool) {
	identity, ok := ctx.Value(requestVerifiedIdentityKey{}).(VerifiedIdentity)
	return identity, ok
}

type BrokerCredentialResolver struct{}

var defaultBrokerCredentialResolver = &BrokerCredentialResolver{}

func ResolveBrokerCredential(ctx context.Context, snapshot Snapshot) (string, error) {
	return defaultBrokerCredentialResolver.Resolve(ctx, snapshot)
}
func (*BrokerCredentialResolver) Resolve(ctx context.Context, snapshot Snapshot) (string, error) {
	if bearer := RequestBrokerBearer(ctx); bearer != "" {
		return bearer, nil
	}
	return brokerToken(snapshot.Profile), nil
}
