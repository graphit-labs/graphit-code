package config

import (
	"context"
	"errors"
	"net/url"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/auth"
)

var authenticationConfigKeys = map[string]struct{}{
	"hub.bucket": {}, "hub.region": {}, "hub.endpoint": {}, "hub.prefix": {},
	"hub.access_key_id": {}, "hub.secret_access_key": {},
	"hub.subject.user": {}, "hub.subject.teams": {}, "mcp.api_key": {},
	"ai.embedding.provider": {}, "ai.embedding.model": {}, "ai.embedding.base_url": {}, "ai.embedding.api_key": {}, "ai.embedding.dimensions": {},
	"ai.rerank.provider": {}, "ai.rerank.model": {}, "ai.rerank.base_url": {}, "ai.rerank.api_key": {},
}

func IsAuthenticationConfigKey(key string) bool {
	_, ok := authenticationConfigKeys[strings.ToLower(strings.TrimSpace(key))]
	return ok
}

// S3Config is the object-store view resolved exclusively from the active account snapshot.
type S3Config struct {
	Bucket, Region, Endpoint, Prefix           string
	AccessKeyID, SecretAccessKey, SessionToken string
	AWSProfile                                 string
	ExpiresAt                                  time.Time
	Refresh                                    func(context.Context) (S3Config, error)
	ResolutionError                            error
}

func (c S3Config) Configured() bool           { return c.Bucket != "" || c.ResolutionError != nil }
func (c S3Config) HasStaticCredentials() bool { return c.AccessKeyID != "" && c.SecretAccessKey != "" }

func HubS3Config() S3Config {
	return hubS3Config(context.Background(), nil)
}

func ProjectS3Config(ctx context.Context, projectID string) S3Config {
	scope := auth.ProjectStorageScope(projectID)
	return hubS3Config(ctx, &scope)
}

func UserS3Config(ctx context.Context) S3Config {
	scope := auth.UserStorageScope()
	return hubS3Config(ctx, &scope)
}

func HubMetadataS3Config(ctx context.Context) S3Config {
	scope := auth.HubStorageScope()
	return hubS3Config(ctx, &scope)
}

// S3ConfigForURI resolves the narrow Broker grant implied by an authoritative
// storage URI. Direct OIDC and Local providers continue to use their one ambient
// configuration.
func S3ConfigForURI(ctx context.Context, storageURI string) S3Config {
	parsed, err := url.Parse(strings.TrimSpace(storageURI))
	if err != nil || strings.ToLower(parsed.Scheme) != "s3" {
		return HubS3Config()
	}
	parts := strings.Split(strings.Trim(parsed.Path, "/"), "/")
	for i := 0; i+1 < len(parts); i++ {
		if parts[i] != "v2" {
			continue
		}
		switch parts[i+1] {
		case "projects":
			if i+2 < len(parts) {
				return ProjectS3Config(ctx, parts[i+2])
			}
		case "users":
			return UserS3Config(ctx)
		case "registry":
			return HubMetadataS3Config(ctx)
		}
	}
	return HubS3Config()
}

func hubS3Config(ctx context.Context, brokerScope *auth.BrokerStorageScope) S3Config {
	snapshot, err := auth.ResolveActive(ctx)
	if err != nil {
		// No account means local-only operation. Every other active-profile failure is
		// preserved so S3 consumers fail closed instead of falling back to local state.
		if strings.Contains(err.Error(), "no active account profile") {
			return S3Config{}
		}
		return S3Config{ResolutionError: err}
	}
	if snapshot.Provider.Type == auth.ProviderBroker {
		if snapshot.Profile.BrokerS3Disabled {
			return S3Config{}
		}
		if brokerScope == nil {
			return S3Config{ResolutionError: errors.New("broker S3 configuration requires a project, user, or Hub metadata scope")}
		}
		credentials, err := auth.ResolveBrokerS3(ctx, *brokerScope)
		if err != nil {
			return S3Config{ResolutionError: err}
		}
		return s3ConfigFromCredentials(credentials, func(refreshCtx context.Context) (S3Config, error) {
			cfg := hubS3Config(refreshCtx, brokerScope)
			if cfg.ResolutionError != nil {
				return S3Config{}, cfg.ResolutionError
			}
			return cfg, nil
		})
	}
	return S3Config{
		Bucket: firstNonEmptyAuth(snapshot.Profile.S3.Bucket, snapshot.Provider.S3.Bucket), Region: firstNonEmptyAuth(snapshot.Profile.S3.Region, snapshot.Provider.S3.Region),
		Endpoint: firstNonEmptyAuth(snapshot.Profile.S3.Endpoint, snapshot.Provider.S3.Endpoint), Prefix: resolvedAuthPrefix(snapshot),
		AccessKeyID: snapshot.Profile.S3.AccessKeyID, SecretAccessKey: snapshot.Profile.S3.SecretAccessKey,
		SessionToken: snapshot.Profile.S3.SessionToken, AWSProfile: snapshot.Profile.S3.AWSProfile,
		ExpiresAt: snapshot.Profile.S3.ExpiresAt, Refresh: refreshHubS3Config,
	}
}

func s3ConfigFromCredentials(credentials auth.S3Credentials, refresh func(context.Context) (S3Config, error)) S3Config {
	prefix := ""
	if len(credentials.Prefixes) == 1 {
		prefix = normalizePrefix(credentials.Prefixes[0])
	}
	return S3Config{Bucket: credentials.Bucket, Region: credentials.Region, Endpoint: credentials.Endpoint, Prefix: prefix,
		AccessKeyID: credentials.AccessKeyID, SecretAccessKey: credentials.SecretAccessKey, SessionToken: credentials.SessionToken,
		AWSProfile: credentials.AWSProfile, ExpiresAt: credentials.ExpiresAt, Refresh: refresh}
}

func refreshHubS3Config(ctx context.Context) (S3Config, error) {
	cfg := hubS3Config(ctx, nil)
	if cfg.ResolutionError != nil {
		return S3Config{}, cfg.ResolutionError
	}
	return cfg, nil
}

func firstNonEmptyAuth(values ...string) string {
	for _, value := range values {
		if strings.TrimSpace(value) != "" {
			return value
		}
	}
	return ""
}
func resolvedAuthPrefix(snapshot auth.Snapshot) string {
	if len(snapshot.Profile.S3.Prefixes) == 1 {
		return normalizePrefix(snapshot.Profile.S3.Prefixes[0])
	}
	return normalizePrefix(snapshot.Provider.S3.Prefix)
}

// ResolveHubS3 keeps the shared store-construction API while intentionally ignoring
// project and inline configuration: authentication state has one source of truth.
func ResolveHubS3(_, _ ConfigMap) S3Config { return HubS3Config() }

func HubBucket() string   { return HubS3Config().Bucket }
func HubRegion() string   { return HubS3Config().Region }
func HubEndpoint() string { return HubS3Config().Endpoint }
func HubPrefix() string   { return HubS3Config().Prefix }

func normalizePrefix(prefix string) string { return strings.Trim(strings.TrimSpace(prefix), "/") }
