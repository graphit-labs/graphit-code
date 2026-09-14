package auth

import (
	"context"
	"errors"
	"fmt"
	"sync"
	"time"
)

// STSS3Manager owns OIDC web-identity grants for this process. Credentials are
// exchanged only for a concrete storage scope and never enter the auth store.
type STSS3Manager struct {
	Exchange      ScopedSTSExchanger
	Now           func() time.Time
	RefreshBefore time.Duration

	mu      sync.Mutex
	entries map[string]S3Credentials
	flights map[string]*brokerS3Flight
}

var defaultSTSS3Manager = &STSS3Manager{}

func ResolveOIDCSTS(ctx context.Context, scope BrokerStorageScope) (S3Credentials, error) {
	snapshot, err := ResolveActive(ctx)
	if err != nil {
		return S3Credentials{}, err
	}
	return defaultSTSS3Manager.Resolve(ctx, snapshot, scope)
}

func (m *STSS3Manager) Resolve(ctx context.Context, snapshot Snapshot, scope BrokerStorageScope) (S3Credentials, error) {
	if snapshot.Provider.Type != ProviderOIDC || snapshot.Provider.STS == nil || (snapshot.Provider.S3.CredentialSource != "sts" && snapshot.Provider.S3.CredentialSource != "") {
		return S3Credentials{}, errors.New("active provider does not use OIDC STS")
	}
	if err := scope.validate(); err != nil {
		return S3Credentials{}, err
	}
	if RequestBrokerBearer(ctx) != "" && !snapshot.Provider.STS.UseAccessToken {
		return S3Credentials{}, errors.New("remote OIDC STS requires --sts-use-access-token; the daemon cannot exchange its own ID token for another caller")
	}
	key := brokerS3CacheKey(snapshot, scope)
	for {
		m.mu.Lock()
		if m.entries == nil {
			m.entries = map[string]S3Credentials{}
		}
		if m.flights == nil {
			m.flights = map[string]*brokerS3Flight{}
		}
		now := time.Now()
		if m.Now != nil {
			now = m.Now()
		}
		margin := m.RefreshBefore
		if margin == 0 {
			margin = 2 * time.Minute
		}
		if credentials, ok := m.entries[key]; ok && usableSTSCredentials(credentials, now, margin) {
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
			exchanger = AWSSTSExchanger{}
		}
		credentials, err := exchanger.ExchangeForScope(ctx, snapshot.Provider, snapshot.Profile, scope)
		if err == nil && !usableSTSCredentials(credentials, now, 0) {
			err = fmt.Errorf("STS returned incomplete or expired temporary S3 credentials")
		}
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
