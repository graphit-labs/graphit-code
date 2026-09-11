package auth

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"strconv"
	"strings"
	"sync"
	"time"
)

type requestBrokerBearerKey struct{}

// WithBrokerBearer binds a verified inbound end-user token to one request. Broker clients prefer
// this token over profile credentials, preventing cross-user identity leakage in the HTTP daemon.
func WithBrokerBearer(ctx context.Context, token string) context.Context {
	return context.WithValue(ctx, requestBrokerBearerKey{}, strings.TrimSpace(token))
}

func RequestBrokerBearer(ctx context.Context) string {
	token, _ := ctx.Value(requestBrokerBearerKey{}).(string)
	return strings.TrimSpace(token)
}

type brokerTokenCacheEntry struct {
	token     string
	expiresAt time.Time
}

type BrokerCredentialResolver struct {
	OIDC *OIDCClient
	Now  func() time.Time

	mu      sync.Mutex
	entries map[string]brokerTokenCacheEntry
}

var defaultBrokerCredentialResolver = &BrokerCredentialResolver{}

func ResolveBrokerCredential(ctx context.Context, snapshot Snapshot) (string, error) {
	return defaultBrokerCredentialResolver.Resolve(ctx, snapshot)
}

func (r *BrokerCredentialResolver) Resolve(ctx context.Context, snapshot Snapshot) (string, error) {
	source := RequestBrokerBearer(ctx)
	if source == "" {
		source = brokerToken(snapshot.Profile)
	}
	if source == "" || snapshot.Provider.Broker == nil {
		return source, nil
	}
	strategy := strings.TrimSpace(snapshot.Provider.Broker.TokenStrategy)
	if strategy == "" || strategy == "relay" {
		return source, nil
	}
	if strategy != "token-exchange" {
		return "", errors.New("unsupported broker token strategy")
	}
	if snapshot.Provider.OIDC == nil {
		return "", errors.New("broker token exchange requires an OIDC provider")
	}
	cacheKey := brokerExchangeCacheKey(snapshot.Provider, source)
	now := r.now()
	r.mu.Lock()
	if entry, ok := r.entries[cacheKey]; ok && entry.expiresAt.After(now.Add(15*time.Second)) {
		r.mu.Unlock()
		return entry.token, nil
	}
	r.mu.Unlock()

	client := r.OIDC
	if client == nil {
		client = NewOIDCClient()
	}
	exchanged, err := client.ExchangeAccessToken(ctx, snapshot.Provider, source)
	if err != nil {
		return "", err
	}
	r.mu.Lock()
	if r.entries == nil {
		r.entries = make(map[string]brokerTokenCacheEntry)
	}
	if len(r.entries) >= 256 {
		for key, entry := range r.entries {
			if !entry.expiresAt.After(now.Add(15 * time.Second)) {
				delete(r.entries, key)
			}
		}
		if len(r.entries) >= 256 {
			for key := range r.entries {
				delete(r.entries, key)
				break
			}
		}
	}
	r.entries[cacheKey] = brokerTokenCacheEntry{token: exchanged.AccessToken, expiresAt: exchanged.ExpiresAt}
	r.mu.Unlock()
	return exchanged.AccessToken, nil
}

func (r *BrokerCredentialResolver) now() time.Time {
	if r.Now != nil {
		return r.Now()
	}
	return time.Now()
}

func brokerExchangeCacheKey(provider Provider, source string) string {
	digest := sha256.Sum256([]byte(provider.Name + "\x00" + strconv.FormatUint(provider.Revision, 10) + "\x00" + provider.Broker.Endpoint + "\x00" + provider.Broker.Audience + "\x00" + provider.Broker.Resource + "\x00" + source))
	return hex.EncodeToString(digest[:])
}
