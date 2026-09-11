package auth

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"regexp"
	"strings"
	"time"
)

type BrokerDiscovery struct {
	Version        string `json:"version"`
	Issuer         string `json:"issuer"`
	Authentication struct {
		Type                string   `json:"type"`
		Issuer              string   `json:"issuer"`
		ClientID            string   `json:"client_id"`
		Scopes              []string `json:"scopes"`
		RedirectURIPath     string   `json:"redirect_uri_path"`
		Audiences           []string `json:"audiences"`
		AccessTokenAudience string   `json:"access_token_audience"`
	} `json:"authentication"`
	Services struct {
		Embeddings *struct {
			Protocol   string `json:"protocol"`
			Path       string `json:"path"`
			Revision   string `json:"revision"`
			Dimensions int    `json:"dimensions"`
			MaxBatch   int    `json:"max_batch"`
		} `json:"embeddings"`
		Rerank *struct {
			Protocol     string `json:"protocol"`
			Path         string `json:"path"`
			Revision     string `json:"revision"`
			MaxDocuments int    `json:"max_documents"`
		} `json:"rerank"`
		S3Credentials *struct {
			Protocol              string `json:"protocol"`
			Path                  string `json:"path"`
			AuthorizationRevision string `json:"authorization_revision"`
		} `json:"s3_credentials"`
		HubAccess *struct {
			Protocol              string `json:"protocol"`
			Path                  string `json:"path"`
			AuthorizationRevision string `json:"authorization_revision"`
		} `json:"hub_access"`
	} `json:"services"`
}

var ErrBrokerS3Unavailable = errors.New("broker does not advertise temporary S3 credentials")
var ErrBrokerHubAccessUnavailable = errors.New("broker does not advertise Hub access resolution")

type BrokerHubSelector struct {
	ID         string `json:"id,omitempty"`
	NamePrefix string `json:"name_prefix,omitempty"`
	All        bool   `json:"all,omitempty"`
}

type BrokerHubAccessResponse struct {
	Version               int                 `json:"v"`
	AuthorizationRevision string              `json:"authorization_revision"`
	Subject               string              `json:"subject"`
	Selectors             []BrokerHubSelector `json:"selectors"`
}

type BrokerHubAccessClient struct {
	HTTP             *http.Client
	Credentials      *BrokerCredentialResolver
	ProviderName     string
	ProviderRevision uint64
}

func NewBrokerHubAccessClient(ctx context.Context, client *http.Client) (*BrokerHubAccessClient, error) {
	snapshot, err := ResolveActive(ctx)
	if err != nil {
		if errors.Is(err, ErrNoActiveProfile) {
			return nil, ErrBrokerHubAccessUnavailable
		}
		return nil, err
	}
	if snapshot.Provider.Broker == nil {
		return nil, ErrBrokerHubAccessUnavailable
	}
	discovery, err := DiscoverBroker(ctx, snapshot.Provider, client)
	if err != nil {
		return nil, err
	}
	capability := discovery.Services.HubAccess
	if capability == nil {
		return nil, ErrBrokerHubAccessUnavailable
	}
	if capability.Protocol != "graphit-hub-access-v1" || !validBrokerPath(capability.Path) || strings.TrimSpace(capability.AuthorizationRevision) == "" {
		return nil, errors.New("broker advertises an incompatible Hub access service")
	}
	return &BrokerHubAccessClient{HTTP: client, ProviderName: snapshot.Provider.Name, ProviderRevision: snapshot.Provider.Revision}, nil
}

func (c *BrokerHubAccessClient) Resolve(ctx context.Context) (BrokerHubAccessResponse, error) {
	var output BrokerHubAccessResponse
	snapshot, err := ResolveActive(ctx)
	if err != nil {
		return output, err
	}
	provider := snapshot.Provider
	if provider.Name != c.ProviderName || provider.Revision != c.ProviderRevision {
		return output, errors.New("active Hub access provider changed; rebuild the store")
	}
	resolver := c.Credentials
	if resolver == nil {
		resolver = defaultBrokerCredentialResolver
	}
	token, err := resolver.Resolve(ctx, snapshot)
	if err != nil {
		return output, fmt.Errorf("resolve broker credential: %w", err)
	}
	if token == "" && !provider.Broker.AllowAnonymous {
		return output, errors.New("active profile has no broker credential and anonymous broker access is disabled")
	}
	discovery, err := DiscoverBroker(ctx, provider, c.client())
	if err != nil {
		return output, err
	}
	capability := discovery.Services.HubAccess
	if capability == nil || capability.Protocol != "graphit-hub-access-v1" || !validBrokerPath(capability.Path) {
		return output, errors.New("broker does not advertise a compatible Hub access service")
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, strings.TrimRight(provider.Broker.Endpoint, "/")+capability.Path, bytes.NewReader([]byte("{}")))
	if err != nil {
		return output, err
	}
	if token != "" {
		req.Header.Set("Authorization", "Bearer "+token)
	}
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Accept", "application/json")
	resp, err := c.client().Do(req)
	if err != nil {
		return output, fmt.Errorf("broker Hub access request: %w", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode/100 != 2 {
		_, _ = io.Copy(io.Discard, io.LimitReader(resp.Body, 4096))
		return output, fmt.Errorf("broker Hub access request returned HTTP %d", resp.StatusCode)
	}
	decoder := json.NewDecoder(io.LimitReader(resp.Body, 1<<20))
	decoder.DisallowUnknownFields()
	if err := decoder.Decode(&output); err != nil {
		return BrokerHubAccessResponse{}, fmt.Errorf("decode broker Hub access response: %w", err)
	}
	if output.Version != 1 || strings.TrimSpace(output.Subject) == "" || strings.TrimSpace(output.AuthorizationRevision) == "" {
		return BrokerHubAccessResponse{}, errors.New("broker returned an invalid Hub access response")
	}
	if capability.AuthorizationRevision != "" && output.AuthorizationRevision != capability.AuthorizationRevision {
		return BrokerHubAccessResponse{}, errors.New("broker Hub authorization revision changed during resolution")
	}
	return output, nil
}

func (c *BrokerHubAccessClient) client() *http.Client {
	if c.HTTP != nil {
		return c.HTTP
	}
	return &http.Client{Timeout: 30 * time.Second}
}

type BrokerCredentialExchanger struct {
	HTTP        *http.Client
	Credentials *BrokerCredentialResolver
}

type BrokerStorageScope struct {
	Kind      string `json:"scope"`
	ProjectID string `json:"project_id,omitempty"`
}

func ProjectStorageScope(projectID string) BrokerStorageScope {
	return BrokerStorageScope{Kind: "project", ProjectID: strings.TrimSpace(projectID)}
}

func UserStorageScope() BrokerStorageScope { return BrokerStorageScope{Kind: "user"} }

func HubStorageScope() BrokerStorageScope { return BrokerStorageScope{Kind: "hub"} }

func (s BrokerStorageScope) cacheKey() string { return s.Kind + "\x00" + s.ProjectID }

var brokerStorageProjectPattern = regexp.MustCompile(`^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$`)

func (s BrokerStorageScope) validate() error {
	switch s.Kind {
	case "project":
		if !brokerStorageProjectPattern.MatchString(s.ProjectID) || s.ProjectID == "." || s.ProjectID == ".." {
			return errors.New("project storage scope requires a safe project ID")
		}
	case "user", "hub":
		if s.ProjectID != "" {
			return fmt.Errorf("%s storage scope cannot select a project", s.Kind)
		}
	default:
		return fmt.Errorf("unsupported broker storage scope %q", s.Kind)
	}
	return nil
}

func (e BrokerCredentialExchanger) ExchangeForScope(ctx context.Context, provider Provider, profile Profile, scope BrokerStorageScope) (S3Credentials, error) {
	if provider.Broker == nil || strings.TrimSpace(provider.Broker.Endpoint) == "" {
		return S3Credentials{}, errors.New("provider has no broker configuration")
	}
	if err := scope.validate(); err != nil {
		return S3Credentials{}, err
	}
	resolver := e.Credentials
	if resolver == nil {
		resolver = defaultBrokerCredentialResolver
	}
	token, err := resolver.Resolve(ctx, Snapshot{Provider: provider, Profile: profile})
	if err != nil {
		return S3Credentials{}, fmt.Errorf("resolve broker credential: %w", err)
	}
	if token == "" && !provider.Broker.AllowAnonymous {
		return S3Credentials{}, errors.New("broker credential exchange requires an authenticated broker token")
	}
	discovery, err := DiscoverBroker(ctx, provider, e.client())
	if err != nil {
		return S3Credentials{}, err
	}
	capability := discovery.Services.S3Credentials
	if capability == nil {
		return S3Credentials{}, ErrBrokerS3Unavailable
	}
	if capability.Protocol != "graphit-s3-credentials-v2" || !validBrokerPath(capability.Path) {
		return S3Credentials{}, errors.New("broker advertises an incompatible temporary S3 credential service")
	}
	requestBody, err := json.Marshal(scope)
	if err != nil {
		return S3Credentials{}, err
	}
	endpoint := strings.TrimRight(provider.Broker.Endpoint, "/") + capability.Path
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, endpoint, bytes.NewReader(requestBody))
	if err != nil {
		return S3Credentials{}, err
	}
	if token != "" {
		req.Header.Set("Authorization", "Bearer "+token)
	}
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Accept", "application/json")
	resp, err := e.client().Do(req)
	if err != nil {
		return S3Credentials{}, fmt.Errorf("broker S3 credential request: %w", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode/100 != 2 {
		_, _ = io.Copy(io.Discard, io.LimitReader(resp.Body, 4096))
		return S3Credentials{}, fmt.Errorf("broker S3 credential request returned HTTP %d", resp.StatusCode)
	}
	var output struct {
		S3Credentials
		Scope     string `json:"scope"`
		ProjectID string `json:"project_id,omitempty"`
	}
	decoder := json.NewDecoder(io.LimitReader(resp.Body, 1<<20))
	decoder.DisallowUnknownFields()
	if err := decoder.Decode(&output); err != nil {
		return S3Credentials{}, fmt.Errorf("decode broker S3 credentials: %w", err)
	}
	if output.Scope != scope.Kind || output.ProjectID != scope.ProjectID {
		return S3Credentials{}, errors.New("broker returned credentials for a different storage scope")
	}
	if err := validateBrokerS3Credentials(output.S3Credentials, capability.AuthorizationRevision, time.Now()); err != nil {
		return S3Credentials{}, err
	}
	output.Prefixes = normalizeS3Prefixes(output.Prefixes)
	return output.S3Credentials, nil
}

func (e BrokerCredentialExchanger) client() *http.Client {
	if e.HTTP != nil {
		return e.HTTP
	}
	return &http.Client{Timeout: 30 * time.Second}
}

func validateBrokerS3Credentials(output S3Credentials, revision string, now time.Time) error {
	if !output.Complete() || strings.TrimSpace(output.SessionToken) == "" || output.ExpiresAt.IsZero() || !output.ExpiresAt.After(now) {
		return errors.New("broker returned incomplete or expired temporary S3 credentials")
	}
	if strings.TrimSpace(output.Bucket) == "" || strings.TrimSpace(output.Region) == "" {
		return errors.New("broker returned S3 credentials without bucket or region")
	}
	if len(normalizeS3Prefixes(output.Prefixes)) != 1 {
		return fmt.Errorf("broker returned %d S3 roots; this client requires exactly one common storage root", len(normalizeS3Prefixes(output.Prefixes)))
	}
	if strings.TrimSpace(output.AuthorizationRevision) == "" || (revision != "" && output.AuthorizationRevision != revision) {
		return errors.New("broker S3 authorization revision changed during credential exchange")
	}
	return nil
}

func normalizeS3Prefixes(prefixes []string) []string {
	out := make([]string, 0, len(prefixes))
	seen := map[string]struct{}{}
	for _, prefix := range prefixes {
		prefix = strings.Trim(strings.TrimSpace(prefix), "/")
		if prefix == "" {
			continue
		}
		if _, exists := seen[prefix]; exists {
			continue
		}
		seen[prefix] = struct{}{}
		out = append(out, prefix)
	}
	return out
}

func validBrokerPath(value string) bool {
	return strings.HasPrefix(value, "/") && !strings.HasPrefix(value, "//") && !strings.Contains(value, "?") && !strings.Contains(value, "#")
}

func DiscoverBroker(ctx context.Context, provider Provider, client *http.Client) (BrokerDiscovery, error) {
	var discovery BrokerDiscovery
	if provider.Broker == nil {
		return discovery, errors.New("provider has no broker configuration")
	}
	if client == nil {
		client = &http.Client{Timeout: 15 * time.Second}
	}
	endpoint := strings.TrimRight(provider.Broker.Endpoint, "/") + "/.well-known/graphit-broker"
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, endpoint, nil)
	if err != nil {
		return discovery, err
	}
	resp, err := client.Do(req)
	if err != nil {
		return discovery, fmt.Errorf("broker discovery: %w", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode/100 != 2 {
		return discovery, fmt.Errorf("broker discovery returned HTTP %d", resp.StatusCode)
	}
	if err := json.NewDecoder(io.LimitReader(resp.Body, 1<<20)).Decode(&discovery); err != nil {
		return discovery, fmt.Errorf("decode broker discovery: %w", err)
	}
	if discovery.Version != "1" {
		return discovery, fmt.Errorf("unsupported broker discovery version %q", discovery.Version)
	}
	configured := strings.TrimRight(strings.TrimSpace(provider.Broker.Endpoint), "/")
	if discovery.Issuer != "" && strings.TrimRight(discovery.Issuer, "/") != configured {
		return discovery, errors.New("broker discovery issuer does not match configured endpoint")
	}
	return discovery, nil
}

func brokerToken(profile Profile) string {
	if profile.BrokerKey != "" {
		return profile.BrokerKey
	}
	if profile.OIDC != nil && profile.OIDC.AccessToken != "" {
		return profile.OIDC.AccessToken
	}
	return ""
}

// ResolveServiceCredential refreshes the active session and returns the credential for one AI service.
func ResolveServiceCredential(ctx context.Context, service string) (Snapshot, string, error) {
	snapshot, err := ResolveActiveOrDefaultLocal(ctx)
	if err != nil {
		return Snapshot{}, "", err
	}
	var mode ServiceMode
	switch service {
	case "embedding":
		mode = snapshot.Provider.AI.Embedding.Mode
	case "rerank":
		mode = snapshot.Provider.AI.Rerank.Mode
	default:
		return Snapshot{}, "", fmt.Errorf("unknown AI service %q", service)
	}
	switch mode {
	case ServiceBroker:
		if snapshot.Profile.Name == "" {
			return Snapshot{}, "", ErrNoActiveProfile
		}
		token, err := ResolveBrokerCredential(ctx, snapshot)
		return snapshot, token, err
	case ServiceDirect:
		if snapshot.Profile.Name == "" {
			return Snapshot{}, "", ErrNoActiveProfile
		}
		if service == "embedding" {
			return snapshot, snapshot.Profile.EmbeddingAPIKey, nil
		}
		return snapshot, snapshot.Profile.RerankAPIKey, nil
	case ServiceLocal, "":
		return snapshot, "", nil
	case ServiceDisabled:
		return snapshot, "", fmt.Errorf("%s service is disabled by the active provider", service)
	default:
		return snapshot, "", fmt.Errorf("unsupported %s service mode %q", service, mode)
	}
}
