package auth

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"sync"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/lockfile"
)

const stateFile = "auth.json"

var ErrNoActiveProfile = errors.New("no active account profile; run `graphit login`")

type Store struct {
	path string
	mu   sync.Mutex
	now  func() time.Time
}

func Open() (*Store, error) {
	dir := brand.GlobalDir()
	if dir == "" {
		return nil, errors.New("cannot resolve global Graphit directory")
	}
	return OpenAt(filepath.Join(dir, stateFile)), nil
}

func OpenAt(path string) *Store { return &Store{path: path, now: time.Now} }

func (s *Store) Path() string { return s.path }

func (s *Store) Load() (State, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.loadUnlocked()
}

func (s *Store) loadUnlocked() (State, error) {
	data, err := os.ReadFile(s.path)
	if errors.Is(err, os.ErrNotExist) {
		return emptyState(), nil
	}
	if err != nil {
		return State{}, fmt.Errorf("read authentication state: %w", err)
	}
	if err := os.Chmod(filepath.Dir(s.path), 0o700); err != nil {
		return State{}, fmt.Errorf("restrict authentication directory: %w", err)
	}
	if err := os.Chmod(s.path, 0o600); err != nil {
		return State{}, fmt.Errorf("restrict authentication state: %w", err)
	}
	var state State
	decoder := json.NewDecoder(bytes.NewReader(data))
	decoder.DisallowUnknownFields()
	if err := decoder.Decode(&state); err != nil {
		return State{}, fmt.Errorf("parse authentication state: %w", err)
	}
	if state.Version != StateVersion {
		return State{}, fmt.Errorf("unsupported authentication state version %d", state.Version)
	}
	if state.Providers == nil {
		state.Providers = map[string]Provider{}
	}
	if state.Profiles == nil {
		state.Profiles = map[string]Profile{}
	}
	return state, nil
}

func (s *Store) update(fn func(*State) error) error {
	return s.updateIfChanged(func(state *State) (bool, error) {
		if err := fn(state); err != nil {
			return false, err
		}
		return true, nil
	})
}

func (s *Store) updateIfChanged(fn func(*State) (bool, error)) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	if err := os.MkdirAll(filepath.Dir(s.path), 0o700); err != nil {
		return fmt.Errorf("create authentication directory: %w", err)
	}
	if err := os.Chmod(filepath.Dir(s.path), 0o700); err != nil {
		return fmt.Errorf("restrict authentication directory: %w", err)
	}
	lock, err := lockfile.Acquire(s.path+".lock", 3*time.Second)
	if err != nil {
		return fmt.Errorf("lock authentication state: %w", err)
	}
	defer lock.Release()
	state, err := s.loadUnlocked()
	if err != nil {
		return err
	}
	changed, err := fn(&state)
	if err != nil {
		return err
	}
	if !changed {
		return nil
	}
	return s.saveUnlocked(state)
}

func (s *Store) saveUnlocked(state State) error {
	stripBrokerS3ForPersistence(&state)
	data, err := json.MarshalIndent(state, "", "  ")
	if err != nil {
		return fmt.Errorf("serialize authentication state: %w", err)
	}
	data = append(data, '\n')
	dir := filepath.Dir(s.path)
	f, err := os.CreateTemp(dir, ".auth-*.tmp")
	if err != nil {
		return fmt.Errorf("create authentication state: %w", err)
	}
	tmp := f.Name()
	defer os.Remove(tmp)
	if err := f.Chmod(0o600); err != nil {
		_ = f.Close()
		return err
	}
	if _, err := f.Write(data); err != nil {
		_ = f.Close()
		return fmt.Errorf("write authentication state: %w", err)
	}
	if err := f.Sync(); err != nil {
		_ = f.Close()
		return fmt.Errorf("sync authentication state: %w", err)
	}
	if err := f.Close(); err != nil {
		return fmt.Errorf("close authentication state: %w", err)
	}
	if err := os.Rename(tmp, s.path); err != nil {
		return fmt.Errorf("publish authentication state: %w", err)
	}
	if err := os.Chmod(s.path, 0o600); err != nil {
		return fmt.Errorf("restrict authentication state: %w", err)
	}
	return nil
}

func (s *Store) AddProvider(provider Provider) error {
	return s.update(func(state *State) error {
		if err := ValidateProvider(provider); err != nil {
			return err
		}
		if _, exists := state.Providers[provider.Name]; exists {
			return fmt.Errorf("provider %q already exists", provider.Name)
		}
		now := s.now().UTC()
		provider.Revision, provider.CreatedAt, provider.UpdatedAt = 1, now, now
		state.Providers[provider.Name] = provider
		return nil
	})
}

func (s *Store) UpdateProvider(provider Provider) error {
	return s.update(func(state *State) error {
		if err := ValidateProvider(provider); err != nil {
			return err
		}
		old, exists := state.Providers[provider.Name]
		if !exists {
			return fmt.Errorf("provider %q does not exist", provider.Name)
		}
		provider.Revision, provider.CreatedAt, provider.UpdatedAt = old.Revision+1, old.CreatedAt, s.now().UTC()
		state.Providers[provider.Name] = provider
		for name, profile := range state.Profiles {
			if profile.Provider == provider.Name {
				profile.ProviderRevision = 0
				profile.S3.SessionToken = ""
				profile.S3.ExpiresAt = time.Time{}
				state.Profiles[name] = profile
			}
		}
		return nil
	})
}

func (s *Store) RemoveProvider(name string, cascade bool) error {
	return s.update(func(state *State) error {
		if _, exists := state.Providers[name]; !exists {
			return fmt.Errorf("provider %q does not exist", name)
		}
		var refs []string
		for profileName, profile := range state.Profiles {
			if profile.Provider == name {
				refs = append(refs, profileName)
			}
		}
		if len(refs) > 0 && !cascade {
			return fmt.Errorf("provider %q is referenced by profiles; pass --cascade --yes to remove them", name)
		}
		for _, profileName := range refs {
			delete(state.Profiles, profileName)
			if state.ActiveProfile == profileName {
				state.ActiveProfile = ""
			}
		}
		delete(state.Providers, name)
		return nil
	})
}

func (s *Store) Login(profile Profile) error {
	return s.update(func(state *State) error {
		if err := ValidateProfile(profile); err != nil {
			return err
		}
		provider, exists := state.Providers[profile.Provider]
		if !exists {
			return fmt.Errorf("provider %q does not exist", profile.Provider)
		}
		switch provider.Type {
		case ProviderLocal:
			if profile.OIDC != nil {
				return errors.New("local profile cannot contain an OIDC session")
			}
			local := provider.Local
			if provider.S3.Bucket != "" && provider.S3.CredentialSource != "broker" && provider.STS == nil && !profile.S3.Complete() && profile.S3.AWSProfile == "" && (local == nil || !local.AllowAWSCredentialChain) {
				return errors.New("local profile requires S3 credentials, an AWS profile, or a provider that allows the AWS credential chain")
			}
			if provider.MCP.Endpoint != "" && profile.MCPKey == "" && (local == nil || !local.AllowDaemonMCPKey) {
				return errors.New("local profile requires an MCP key for the configured endpoint")
			}
			if (provider.AI.Embedding.Mode == ServiceBroker || provider.AI.Rerank.Mode == ServiceBroker) && profile.BrokerKey == "" && (provider.Broker == nil || !provider.Broker.AllowAnonymous) {
				return errors.New("local profile requires a broker key for broker capabilities")
			}
		case ProviderOIDC:
			if profile.OIDC == nil || profile.OIDC.AccessToken == "" || profile.OIDC.IDToken == "" || profile.Issuer == "" || profile.Subject == "" {
				return errors.New("OIDC profile requires a verified identity and token session")
			}
		case ProviderBroker:
			if profile.OIDC == nil || profile.OIDC.AccessToken == "" || profile.OIDC.IDToken == "" || profile.Issuer == "" || profile.Subject == "" {
				return errors.New("broker profile requires a verified identity and OIDC token session")
			}
			if profile.BrokerKey != "" {
				return errors.New("broker profile cannot contain a static broker key")
			}
			profile.S3 = S3Credentials{}
		}
		if provider.AI.Embedding.Mode == ServiceDirect && profile.EmbeddingAPIKey == "" {
			return errors.New("profile requires an embedding API key for direct mode")
		}
		if provider.AI.Rerank.Mode == ServiceDirect && profile.RerankAPIKey == "" {
			return errors.New("profile requires a rerank API key for direct mode")
		}
		if profile.ProviderRevision == 0 {
			profile.ProviderRevision = provider.Revision
		}
		if profile.ProviderRevision != provider.Revision {
			return fmt.Errorf("profile was authenticated against stale provider revision")
		}
		now := s.now().UTC()
		if old, exists := state.Profiles[profile.Name]; exists {
			profile.CreatedAt = old.CreatedAt
		} else {
			profile.CreatedAt = now
		}
		profile.UpdatedAt = now
		state.Profiles[profile.Name] = profile
		state.ActiveProfile = profile.Name
		return nil
	})
}

func stripBrokerS3ForPersistence(state *State) {
	for name, profile := range state.Profiles {
		provider, ok := state.Providers[profile.Provider]
		if !ok || provider.Type != ProviderBroker || profile.S3.Empty() {
			continue
		}
		profile.S3 = S3Credentials{}
		state.Profiles[name] = profile
	}
}

func (s *Store) UseProfile(name string) error {
	return s.update(func(state *State) error {
		profile, exists := state.Profiles[name]
		if !exists {
			return fmt.Errorf("profile %q does not exist", name)
		}
		provider, exists := state.Providers[profile.Provider]
		if !exists || profile.ProviderRevision != provider.Revision {
			return fmt.Errorf("profile %q must log in again because its provider changed", name)
		}
		state.ActiveProfile = name
		return nil
	})
}

func (s *Store) Logout(name string) error {
	return s.update(func(state *State) error {
		if name == "" {
			name = state.ActiveProfile
		}
		if name == "" {
			return errors.New("no active profile")
		}
		if _, exists := state.Profiles[name]; !exists {
			return fmt.Errorf("profile %q does not exist", name)
		}
		delete(state.Profiles, name)
		if state.ActiveProfile == name {
			state.ActiveProfile = ""
		}
		return nil
	})
}

// UpdateActive replaces the active profile only if its provider revision still matches.
// It is used for token and STS renewals so readers never observe a partially refreshed session.
func (s *Store) UpdateActive(profile Profile) error {
	return s.update(func(state *State) error {
		if state.ActiveProfile == "" || state.ActiveProfile != profile.Name {
			return errors.New("active profile changed while refreshing credentials")
		}
		provider, exists := state.Providers[profile.Provider]
		if !exists || provider.Revision != profile.ProviderRevision {
			return errors.New("provider changed while refreshing credentials")
		}
		profile.CreatedAt = state.Profiles[profile.Name].CreatedAt
		profile.UpdatedAt = s.now().UTC()
		state.Profiles[profile.Name] = profile
		return nil
	})
}

func (s *Store) Active() (Snapshot, error) {
	state, err := s.Load()
	if err != nil {
		return Snapshot{}, err
	}
	return activeSnapshot(state)
}

func activeSnapshot(state State) (Snapshot, error) {
	if state.ActiveProfile == "" {
		return Snapshot{}, ErrNoActiveProfile
	}
	profile, ok := state.Profiles[state.ActiveProfile]
	if !ok {
		return Snapshot{}, errors.New("active account profile is missing")
	}
	provider, ok := state.Providers[profile.Provider]
	if !ok {
		return Snapshot{}, fmt.Errorf("provider %q for active profile is missing", profile.Provider)
	}
	if profile.ProviderRevision != provider.Revision {
		return Snapshot{}, fmt.Errorf("active profile %q must log in again because provider %q changed", profile.Name, provider.Name)
	}
	return Snapshot{Provider: provider, Profile: profile}, nil
}

// EnsureDefaultLocalProvider materializes the canonical local provider without creating a
// profile. Existing explicit service settings are preserved; only omitted local defaults are
// filled in.
func (s *Store) EnsureDefaultLocalProvider() (provider Provider, created bool, err error) {
	err = s.updateIfChanged(func(state *State) (bool, error) {
		var changed bool
		provider, created, changed, err = s.materializeDefaultLocalProvider(state)
		return changed, err
	})
	return provider, created, err
}

// ActiveOrDefaultLocal returns the active profile when one is selected. Otherwise it atomically
// ensures and returns the real persisted local provider with an empty profile.
func (s *Store) ActiveOrDefaultLocal() (snapshot Snapshot, err error) {
	err = s.updateIfChanged(func(state *State) (bool, error) {
		if state.ActiveProfile != "" {
			var activeErr error
			snapshot, activeErr = activeSnapshot(*state)
			return false, activeErr
		}
		provider, _, changed, materializeErr := s.materializeDefaultLocalProvider(state)
		if materializeErr != nil {
			return false, materializeErr
		}
		snapshot = Snapshot{Provider: provider}
		return changed, nil
	})
	return snapshot, err
}

func (s *Store) materializeDefaultLocalProvider(state *State) (Provider, bool, bool, error) {
	provider, exists := state.Providers[DefaultLocalProviderName]
	if !exists {
		provider = DefaultLocalProvider()
		now := s.now().UTC()
		provider.Revision, provider.CreatedAt, provider.UpdatedAt = 1, now, now
		state.Providers[provider.Name] = provider
		return provider, true, true, nil
	}
	changed := false
	if provider.Type == "" {
		provider.Type = ProviderLocal
		changed = true
	} else if provider.Type != ProviderLocal {
		return Provider{}, false, false, fmt.Errorf("default provider %q exists with type %q; remove or rename it before using the local fallback", DefaultLocalProviderName, provider.Type)
	}

	changed = changed || provider.Name != DefaultLocalProviderName || provider.Local == nil
	provider.Name = DefaultLocalProviderName
	if provider.Local == nil {
		provider.Local = &LocalConfig{}
	}
	services := []*AIServiceConfig{&provider.AI.Embedding, &provider.AI.Rerank}
	for _, service := range services {
		if service.Mode == "" {
			service.Mode = ServiceLocal
			changed = true
		}
		if service.Mode == ServiceLocal && service.ONNX == nil {
			execution := DefaultONNXExecutionConfig()
			service.ONNX = &execution
			changed = true
		}
	}
	if err := ValidateProvider(provider); err != nil {
		return Provider{}, false, false, fmt.Errorf("default provider %q is invalid: %w", DefaultLocalProviderName, err)
	}
	if !changed {
		return provider, false, false, nil
	}
	oldRevision := provider.Revision
	if provider.Revision == 0 {
		provider.Revision = 1
	} else {
		provider.Revision++
	}
	now := s.now().UTC()
	if provider.CreatedAt.IsZero() {
		provider.CreatedAt = now
	}
	provider.UpdatedAt = now
	state.Providers[DefaultLocalProviderName] = provider
	for name, profile := range state.Profiles {
		if profile.Provider == DefaultLocalProviderName && profile.ProviderRevision == oldRevision {
			profile.ProviderRevision = provider.Revision
			state.Profiles[name] = profile
		}
	}
	return provider, false, true, nil
}
