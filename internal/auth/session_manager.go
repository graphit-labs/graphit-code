package auth

import (
	"context"
	"errors"
	"fmt"
	"sync"
	"time"
)

type SessionManager struct {
	Store         *Store
	OIDC          *OIDCClient
	STS           STSExchanger
	RefreshBefore time.Duration
	mu            sync.Mutex
}

var resolveActiveMu sync.Mutex

// ResolveActive returns one coherent, refreshed view of the globally active profile.
func ResolveActive(ctx context.Context) (Snapshot, error) {
	resolveActiveMu.Lock()
	defer resolveActiveMu.Unlock()
	store, err := Open()
	if err != nil {
		return Snapshot{}, err
	}
	return (&SessionManager{Store: store, OIDC: NewOIDCClient(), STS: AWSSTSExchanger{}}).Active(ctx)
}

// ResolveActiveOrDefaultLocal refreshes an active account session when present. With no active
// profile, it persists and returns the canonical local provider instead.
func ResolveActiveOrDefaultLocal(ctx context.Context) (Snapshot, error) {
	snapshot, err := ResolveActive(ctx)
	if err == nil || !errors.Is(err, ErrNoActiveProfile) {
		return snapshot, err
	}
	store, openErr := Open()
	if openErr != nil {
		return Snapshot{}, openErr
	}
	return store.ActiveOrDefaultLocal()
}

func (m *SessionManager) Active(ctx context.Context) (Snapshot, error) {
	m.mu.Lock()
	defer m.mu.Unlock()
	snapshot, err := m.Store.Active()
	if err != nil {
		return Snapshot{}, err
	}
	profile, changed := snapshot.Profile, false
	margin := m.RefreshBefore
	if margin == 0 {
		margin = 2 * time.Minute
	}
	if profile.OIDC != nil && !profile.OIDC.ExpiresAt.IsZero() && time.Until(profile.OIDC.ExpiresAt) <= margin {
		client := m.OIDC
		if client == nil {
			client = NewOIDCClient()
		}
		loginProvider := snapshot.Provider
		if loginProvider.Type == ProviderBroker {
			loginProvider, err = BrokerOIDCProvider(ctx, loginProvider, client.client())
			if err != nil {
				return Snapshot{}, fmt.Errorf("resolve broker OIDC provider: %w", err)
			}
		}
		refreshed, err := client.Refresh(ctx, loginProvider, profile)
		if err != nil {
			return Snapshot{}, fmt.Errorf("refresh OIDC session: %w", err)
		}
		refreshed.Name = profile.Name
		refreshed.CreatedAt = profile.CreatedAt
		refreshed.BrokerKey = profile.BrokerKey
		refreshed.EmbeddingAPIKey = profile.EmbeddingAPIKey
		refreshed.RerankAPIKey = profile.RerankAPIKey
		refreshed.S3 = profile.S3
		refreshed.BrokerS3Disabled = profile.BrokerS3Disabled
		profile = refreshed
		changed = true
	}
	needsS3Refresh := !profile.S3.Complete() || (!profile.S3.ExpiresAt.IsZero() && time.Until(profile.S3.ExpiresAt) <= margin)
	if snapshot.Provider.STS != nil && needsS3Refresh {
		exchanger := m.STS
		if exchanger == nil {
			exchanger = AWSSTSExchanger{}
		}
		creds, err := exchanger.Exchange(ctx, snapshot.Provider, profile)
		if err != nil {
			return Snapshot{}, fmt.Errorf("exchange OIDC identity for S3 credentials: %w", err)
		}
		profile.S3 = creds
		changed = true
	}
	if changed {
		if err := m.Store.UpdateActive(profile); err != nil {
			return Snapshot{}, err
		}
		snapshot.Profile = profile
	}
	return snapshot, nil
}
