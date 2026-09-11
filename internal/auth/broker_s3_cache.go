package auth

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"sync"
	"time"
)

type BrokerScopedExchanger interface {
	ExchangeForScope(context.Context, Provider, Profile, BrokerStorageScope) (S3Credentials, error)
}

type brokerS3Flight struct {
	done chan struct{}
	err  error
}

// BrokerS3Manager owns temporary Broker grants for this process. Nothing in this
// cache has a serialization path: auth.json stores the OIDC session and the
// non-secret capability state, while each storage scope is exchanged on demand.
type BrokerS3Manager struct {
	Exchange      BrokerScopedExchanger
	Now           func() time.Time
	RefreshBefore time.Duration

	mu      sync.Mutex
	entries map[string]S3Credentials
	flights map[string]*brokerS3Flight
}

var defaultBrokerS3Manager = &BrokerS3Manager{}

func ResolveBrokerS3(ctx context.Context, scope BrokerStorageScope) (S3Credentials, error) {
	snapshot, err := ResolveActive(ctx)
	if err != nil {
		return S3Credentials{}, err
	}
	return defaultBrokerS3Manager.Resolve(ctx, snapshot, scope)
}

func (m *BrokerS3Manager) Resolve(ctx context.Context, snapshot Snapshot, scope BrokerStorageScope) (S3Credentials, error) {
	if snapshot.Provider.Type != ProviderBroker || snapshot.Provider.Broker == nil {
		return S3Credentials{}, errorsForBrokerScope("active provider is not a Broker provider")
	}
	if snapshot.Profile.BrokerS3Disabled {
		return S3Credentials{}, ErrBrokerS3Unavailable
	}
	if err := scope.validate(); err != nil {
		return S3Credentials{}, err
	}
	key := brokerS3CacheKey(snapshot, scope)
	for {
		m.mu.Lock()
		m.initLocked()
		if credentials, ok := m.entries[key]; ok && m.usable(credentials) {
			m.mu.Unlock()
			return cloneS3Credentials(credentials), nil
		}
		if flight := m.flights[key]; flight != nil {
			done := flight.done
			m.mu.Unlock()
			select {
			case <-ctx.Done():
				return S3Credentials{}, ctx.Err()
			case <-done:
				if flight.err != nil {
					return S3Credentials{}, flight.err
				}
				continue
			}
		}
		flight := &brokerS3Flight{done: make(chan struct{})}
		m.flights[key] = flight
		m.mu.Unlock()

		exchanger := m.Exchange
		if exchanger == nil {
			exchanger = BrokerCredentialExchanger{}
		}
		credentials, err := exchanger.ExchangeForScope(ctx, snapshot.Provider, snapshot.Profile, scope)

		m.mu.Lock()
		if err == nil {
			m.entries[key] = cloneS3Credentials(credentials)
		}
		flight.err = err
		delete(m.flights, key)
		close(flight.done)
		m.mu.Unlock()
		if err != nil {
			return S3Credentials{}, err
		}
		return cloneS3Credentials(credentials), nil
	}
}

func (m *BrokerS3Manager) initLocked() {
	if m.entries == nil {
		m.entries = map[string]S3Credentials{}
	}
	if m.flights == nil {
		m.flights = map[string]*brokerS3Flight{}
	}
}

func (m *BrokerS3Manager) usable(credentials S3Credentials) bool {
	now := time.Now()
	if m.Now != nil {
		now = m.Now()
	}
	margin := m.RefreshBefore
	if margin == 0 {
		margin = 2 * time.Minute
	}
	return credentials.Complete() && !credentials.ExpiresAt.IsZero() && credentials.ExpiresAt.After(now.Add(margin))
}

func brokerS3CacheKey(snapshot Snapshot, scope BrokerStorageScope) string {
	return BrokerStorageIdentity(snapshot) + "\x00" + scope.cacheKey()
}

// BrokerStorageIdentity changes whenever the provider, profile identity, or
// authenticated OIDC session changes. Long-lived stores use it to discard S3
// clients that may still hold an AWS SDK credential cache for the old session.
func BrokerStorageIdentity(snapshot Snapshot) string {
	session := ""
	endpoint := ""
	if snapshot.Provider.Broker != nil {
		endpoint = snapshot.Provider.Broker.Endpoint
	}
	if snapshot.Profile.OIDC != nil {
		digest := sha256.Sum256([]byte(snapshot.Profile.OIDC.IDToken + "\x00" + snapshot.Profile.OIDC.AccessToken))
		session = hex.EncodeToString(digest[:16])
	}
	return fmt.Sprintf("%s\x00%s\x00%d\x00%s\x00%s\x00%s\x00%s", snapshot.Provider.Name,
		endpoint, snapshot.Provider.Revision, snapshot.Profile.Name,
		snapshot.Profile.Issuer, snapshot.Profile.Subject, session)
}

func cloneS3Credentials(credentials S3Credentials) S3Credentials {
	credentials.Prefixes = append([]string(nil), credentials.Prefixes...)
	return credentials
}

func errorsForBrokerScope(message string) error { return fmt.Errorf("broker S3 scope: %s", message) }
