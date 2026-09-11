// Package auth owns reusable authentication providers and isolated account profiles.
package auth

import (
	"errors"
	"fmt"
	"net"
	"net/url"
	"sort"
	"strings"
	"time"
)

const StateVersion = 3

type ProviderType string

const (
	ProviderLocal  ProviderType = "local"
	ProviderOIDC   ProviderType = "oidc"
	ProviderBroker ProviderType = "broker"
)

type Provider struct {
	Name      string        `json:"name"`
	Type      ProviderType  `json:"type"`
	Revision  uint64        `json:"revision"`
	Local     *LocalConfig  `json:"local,omitempty"`
	OIDC      *OIDCConfig   `json:"oidc,omitempty"`
	MCP       MCPConfig     `json:"mcp,omitempty"`
	Broker    *BrokerConfig `json:"broker,omitempty"`
	AI        AIConfig      `json:"ai,omitempty"`
	S3        S3Config      `json:"s3,omitempty"`
	STS       *STSConfig    `json:"sts,omitempty"`
	CreatedAt time.Time     `json:"created_at"`
	UpdatedAt time.Time     `json:"updated_at"`
}

type ServiceMode string

const (
	ServiceLocal    ServiceMode = "local"
	ServiceDirect   ServiceMode = "direct"
	ServiceBroker   ServiceMode = "broker"
	ServiceDisabled ServiceMode = "disabled"
)

type BrokerConfig struct {
	Endpoint              string `json:"endpoint"`
	Audience              string `json:"audience,omitempty"`
	Resource              string `json:"resource,omitempty"`
	TokenStrategy         string `json:"token_strategy,omitempty"`
	TokenExchangeEndpoint string `json:"token_exchange_endpoint,omitempty"`
	AllowAnonymous        bool   `json:"allow_anonymous,omitempty"`
}

type AIConfig struct {
	Embedding AIServiceConfig `json:"embedding,omitempty"`
	Rerank    AIServiceConfig `json:"rerank,omitempty"`
}

type AIServiceConfig struct {
	Mode       ServiceMode          `json:"mode,omitempty"`
	Protocol   string               `json:"protocol,omitempty"`
	Endpoint   string               `json:"endpoint,omitempty"`
	Model      string               `json:"model,omitempty"`
	Dimensions int                  `json:"dimensions,omitempty"`
	ONNX       *ONNXExecutionConfig `json:"onnx,omitempty"`
}

type LocalConfig struct {
	AllowAWSCredentialChain bool `json:"allow_aws_credential_chain,omitempty"`
	AllowDaemonMCPKey       bool `json:"allow_daemon_mcp_key,omitempty"`
}

type OIDCConfig struct {
	Issuer            string            `json:"issuer"`
	ClientID          string            `json:"client_id"`
	ClientSecret      string            `json:"client_secret,omitempty"`
	TokenAuthMethod   string            `json:"token_auth_method,omitempty"`
	Scopes            []string          `json:"scopes,omitempty"`
	RedirectURI       string            `json:"redirect_uri,omitempty"`
	RedirectURIPath   string            `json:"redirect_uri_path,omitempty"`
	UsernameClaim     string            `json:"username_claim,omitempty"`
	OrganizationClaim string            `json:"organization_claim,omitempty"`
	TeamsClaim        string            `json:"teams_claim,omitempty"`
	AuthParams        map[string]string `json:"auth_params,omitempty"`
}

type MCPConfig struct {
	Endpoint string `json:"endpoint,omitempty"`
	Audience string `json:"audience,omitempty"`
	Resource string `json:"resource,omitempty"`
}

// S3Config describes the stable object-store location. Authentication material
// is kept on Profile.S3 and may be renewed without changing the provider.
type S3Config struct {
	Bucket           string `json:"bucket,omitempty"`
	Region           string `json:"region,omitempty"`
	Endpoint         string `json:"endpoint,omitempty"`
	Prefix           string `json:"prefix,omitempty"`
	CredentialSource string `json:"credential_source,omitempty"`
}

type STSConfig struct {
	Endpoint        string `json:"endpoint,omitempty"`
	RoleARN         string `json:"role_arn"`
	RoleSessionName string `json:"role_session_name,omitempty"`
	DurationSeconds int32  `json:"duration_seconds,omitempty"`
	UseAccessToken  bool   `json:"use_access_token,omitempty"`
}

type Profile struct {
	Name             string        `json:"name"`
	Provider         string        `json:"provider"`
	ProviderRevision uint64        `json:"provider_revision"`
	Issuer           string        `json:"issuer,omitempty"`
	Subject          string        `json:"subject,omitempty"`
	Username         string        `json:"username"`
	Organization     string        `json:"organization,omitempty"`
	Teams            []string      `json:"teams,omitempty"`
	MCPKey           string        `json:"mcp_key,omitempty"`
	BrokerKey        string        `json:"broker_key,omitempty"`
	EmbeddingAPIKey  string        `json:"embedding_api_key,omitempty"`
	RerankAPIKey     string        `json:"rerank_api_key,omitempty"`
	OIDC             *OIDCSession  `json:"oidc,omitempty"`
	S3               S3Credentials `json:"s3,omitempty"`
	BrokerS3Disabled bool          `json:"broker_s3_disabled,omitempty"`
	CreatedAt        time.Time     `json:"created_at"`
	UpdatedAt        time.Time     `json:"updated_at"`
}

type OIDCSession struct {
	AccessToken  string    `json:"access_token"`
	RefreshToken string    `json:"refresh_token,omitempty"`
	IDToken      string    `json:"id_token,omitempty"`
	TokenType    string    `json:"token_type,omitempty"`
	ExpiresAt    time.Time `json:"expires_at,omitempty"`
}

// S3Credentials is the complete temporary storage grant returned by STS or the
// broker. Prefixes are authorization metadata; the first prefix is the common
// object-store root used by Graphit clients.
type S3Credentials struct {
	AccessKeyID           string    `json:"access_key_id,omitempty"`
	SecretAccessKey       string    `json:"secret_access_key,omitempty"`
	SessionToken          string    `json:"session_token,omitempty"`
	ExpiresAt             time.Time `json:"expires_at,omitempty"`
	AWSProfile            string    `json:"aws_profile,omitempty"`
	Bucket                string    `json:"bucket,omitempty"`
	Region                string    `json:"region,omitempty"`
	Endpoint              string    `json:"endpoint,omitempty"`
	Prefixes              []string  `json:"prefixes,omitempty"`
	AuthorizationRevision string    `json:"authorization_revision,omitempty"`
}

func (c S3Credentials) Complete() bool {
	return strings.TrimSpace(c.AccessKeyID) != "" && strings.TrimSpace(c.SecretAccessKey) != ""
}

// Empty reports whether the profile has never received an object-store grant.
// It lets Broker profiles whose server intentionally disables S3 remain local,
// while partially populated or previously issued grants continue to fail closed.
func (c S3Credentials) Empty() bool {
	return strings.TrimSpace(c.AccessKeyID) == "" &&
		strings.TrimSpace(c.SecretAccessKey) == "" &&
		strings.TrimSpace(c.SessionToken) == "" &&
		c.ExpiresAt.IsZero() &&
		strings.TrimSpace(c.AWSProfile) == "" &&
		strings.TrimSpace(c.Bucket) == "" &&
		strings.TrimSpace(c.Region) == "" &&
		strings.TrimSpace(c.Endpoint) == "" &&
		len(c.Prefixes) == 0 &&
		strings.TrimSpace(c.AuthorizationRevision) == ""
}

type State struct {
	Version       int                 `json:"version"`
	ActiveProfile string              `json:"active_profile,omitempty"`
	Providers     map[string]Provider `json:"providers"`
	Profiles      map[string]Profile  `json:"profiles"`
}

func emptyState() State {
	return State{Version: StateVersion, Providers: map[string]Provider{}, Profiles: map[string]Profile{}}
}

type Snapshot struct {
	Provider Provider
	Profile  Profile
}

func (s State) ProviderNames() []string {
	names := make([]string, 0, len(s.Providers))
	for name := range s.Providers {
		names = append(names, name)
	}
	sort.Strings(names)
	return names
}

func (s State) ProfileNames() []string {
	names := make([]string, 0, len(s.Profiles))
	for name := range s.Profiles {
		names = append(names, name)
	}
	sort.Strings(names)
	return names
}

func ValidateProvider(p Provider) error {
	p.Name = strings.TrimSpace(p.Name)
	if p.Name == "" {
		return errors.New("provider name is required")
	}
	if strings.ContainsAny(p.Name, "\r\n\t/") {
		return fmt.Errorf("invalid provider name %q", p.Name)
	}
	switch p.Type {
	case ProviderLocal:
		if p.Local == nil {
			p.Local = &LocalConfig{}
		}
	case ProviderOIDC:
		if p.OIDC == nil {
			return errors.New("OIDC provider configuration is required")
		}
		if err := validateServiceURL(p.OIDC.Issuer, "OIDC issuer"); err != nil {
			return err
		}
		if strings.TrimSpace(p.OIDC.ClientID) == "" {
			return errors.New("OIDC client ID is required")
		}
		if strings.TrimSpace(p.OIDC.UsernameClaim) == "" {
			return errors.New("OIDC username claim is required")
		}
		if method := p.OIDC.TokenAuthMethod; method != "" && method != "none" && method != "client_secret_post" && method != "client_secret_basic" {
			return fmt.Errorf("unsupported OIDC token auth method %q", method)
		}
		for key := range p.OIDC.AuthParams {
			key = strings.TrimSpace(key)
			if key == "" {
				return errors.New("OIDC authorization parameter name is required")
			}
			if reservedOIDCAuthParameter(key) {
				return fmt.Errorf("OIDC authorization parameter %q is reserved", key)
			}
		}
	case ProviderBroker:
		if p.Broker == nil {
			return errors.New("broker provider configuration is required")
		}
		if p.Local != nil || p.OIDC != nil {
			return errors.New("broker provider cannot contain local or upstream OIDC configuration")
		}
	default:
		return fmt.Errorf("unsupported provider type %q", p.Type)
	}
	if p.STS != nil {
		if p.Type != ProviderOIDC {
			return errors.New("STS web identity exchange is supported only for OIDC providers")
		}
		if strings.TrimSpace(p.STS.RoleARN) == "" {
			return errors.New("STS role ARN is required")
		}
		if p.STS.Endpoint != "" {
			if err := validateServiceURL(p.STS.Endpoint, "STS endpoint"); err != nil {
				return err
			}
		}
	}
	if p.Broker != nil {
		if err := validateServiceURL(p.Broker.Endpoint, "broker endpoint"); err != nil {
			return err
		}
		strategy := strings.TrimSpace(p.Broker.TokenStrategy)
		if strategy == "" {
			strategy = "relay"
		}
		if strategy != "relay" && strategy != "token-exchange" {
			return fmt.Errorf("unsupported broker token strategy %q", p.Broker.TokenStrategy)
		}
		if p.Type == ProviderBroker && strategy != "relay" {
			return errors.New("broker authentication provider uses broker-issued tokens and requires relay strategy")
		}
		if p.Type == ProviderBroker && p.Broker.AllowAnonymous {
			return errors.New("broker authentication provider cannot allow anonymous login")
		}
		if p.Type == ProviderBroker && (strings.TrimSpace(p.Broker.Audience) != "" || strings.TrimSpace(p.Broker.Resource) != "" || strings.TrimSpace(p.Broker.TokenExchangeEndpoint) != "") {
			return errors.New("broker authentication provider discovers its token contract and cannot configure audience, resource, or token exchange")
		}
		if strategy == "token-exchange" {
			if p.Type != ProviderOIDC {
				return errors.New("broker token exchange requires an OIDC provider")
			}
			if strings.TrimSpace(p.Broker.Audience) == "" && strings.TrimSpace(p.Broker.Resource) == "" {
				return errors.New("broker token exchange requires broker audience or resource")
			}
			if p.Broker.TokenExchangeEndpoint != "" {
				if err := validateServiceURL(p.Broker.TokenExchangeEndpoint, "broker token exchange endpoint"); err != nil {
					return err
				}
			}
			if strings.TrimSpace(p.MCP.Audience) == "" {
				return errors.New("broker token exchange requires an MCP audience for the subject access token")
			}
		}
		if p.AI.Embedding.Mode != ServiceBroker || p.AI.Rerank.Mode != ServiceBroker {
			return errors.New("broker configuration requires embedding and rerank modes to be broker")
		}
	}
	if err := validateAIService(p.AI.Embedding, true); err != nil {
		return fmt.Errorf("embedding: %w", err)
	}
	if err := validateAIService(p.AI.Rerank, false); err != nil {
		return fmt.Errorf("rerank: %w", err)
	}
	if (p.AI.Embedding.Mode == ServiceBroker || p.AI.Rerank.Mode == ServiceBroker) && p.Broker == nil {
		return errors.New("broker AI mode requires broker configuration")
	}
	source := strings.TrimSpace(p.S3.CredentialSource)
	switch source {
	case "", "login", "aws-chain", "sts", "broker":
	default:
		return fmt.Errorf("unsupported S3 credential source %q", p.S3.CredentialSource)
	}
	switch p.Type {
	case ProviderLocal:
		if source == "sts" || source == "broker" {
			return fmt.Errorf("local provider cannot use S3 credential source %q", source)
		}
	case ProviderOIDC:
		if source != "" && source != "sts" {
			return fmt.Errorf("OIDC provider must use STS for S3 credentials, got %q", source)
		}
		if strings.TrimSpace(p.S3.Bucket) != "" && p.STS == nil {
			return errors.New("OIDC provider with S3 storage requires STS configuration")
		}
	case ProviderBroker:
		if source != "" && source != "broker" {
			return fmt.Errorf("broker provider must obtain S3 credentials from the broker, got %q", source)
		}
		if p.STS != nil {
			return errors.New("broker provider obtains temporary S3 credentials from the broker and cannot configure direct STS")
		}
	}
	if p.Type == ProviderOIDC && p.Broker != nil && p.Broker.TokenStrategy != "token-exchange" && p.Broker.Audience != "" && p.MCP.Audience != "" && p.Broker.Audience != p.MCP.Audience {
		return errors.New("MCP and broker audiences must match while the profile uses one OIDC access-token session")
	}
	if p.Type == ProviderOIDC && p.Broker != nil && p.Broker.TokenStrategy != "token-exchange" && p.Broker.Resource != "" && p.MCP.Resource != "" && p.Broker.Resource != p.MCP.Resource {
		return errors.New("MCP and broker resources must match while direct token relay is enabled")
	}
	return nil
}

func validateAIService(service AIServiceConfig, embedding bool) error {
	mode := service.Mode
	if mode == "" {
		mode = ServiceLocal
	}
	switch mode {
	case ServiceLocal:
		if service.Protocol != "" || service.Endpoint != "" || service.Model != "" || service.Dimensions != 0 {
			return errors.New("local mode does not accept protocol, endpoint, model, or dimensions")
		}
		if service.ONNX != nil {
			if err := validateCurrentPlatformONNXExecution(*service.ONNX); err != nil {
				return fmt.Errorf("ONNX execution: %w", err)
			}
		}
	case ServiceDisabled:
		if service.Protocol != "" || service.Endpoint != "" || service.Model != "" || service.Dimensions != 0 || service.ONNX != nil {
			return errors.New("disabled mode does not accept protocol, endpoint, model, dimensions, or ONNX execution")
		}
	case ServiceBroker:
		if service.Protocol != "" || service.Endpoint != "" || service.Model != "" || service.Dimensions != 0 || service.ONNX != nil {
			return errors.New("broker mode uses broker discovery and does not accept protocol, endpoint, model, dimensions, or ONNX execution")
		}
	case ServiceDirect:
		if service.ONNX != nil {
			return errors.New("direct mode does not accept ONNX execution")
		}
		if err := validateServiceURL(service.Endpoint, "direct endpoint"); err != nil {
			return err
		}
		if strings.TrimSpace(service.Protocol) == "" || strings.TrimSpace(service.Model) == "" {
			return errors.New("direct mode requires protocol and model")
		}
		protocol := strings.ToLower(strings.TrimSpace(service.Protocol))
		if embedding {
			switch protocol {
			case "openai", "openai-compatible", "openai-embeddings-v1", "cohere", "voyage", "google":
			default:
				return fmt.Errorf("unsupported direct embedding protocol %q", service.Protocol)
			}
		} else {
			switch protocol {
			case "cohere", "cohere-v2", "voyage", "voyage-v1", "jina", "jina-v1":
				if service.Dimensions != 0 {
					return errors.New("native direct rerank mode does not accept dimensions")
				}
			case "openai", "openai-compatible", "openai-embeddings-v1", "google":
				if service.Dimensions <= 0 {
					return errors.New("embedding-simulated direct rerank mode requires positive dimensions")
				}
			default:
				return fmt.Errorf("unsupported direct rerank protocol %q", service.Protocol)
			}
		}
		if embedding && service.Dimensions <= 0 {
			return errors.New("direct embedding mode requires positive dimensions")
		}
	default:
		return fmt.Errorf("unsupported mode %q", mode)
	}
	return nil
}

func validateServiceURL(raw, name string) error {
	raw = strings.TrimSpace(raw)
	u, err := url.Parse(raw)
	if err != nil || u.Host == "" || (u.Scheme != "http" && u.Scheme != "https") {
		return fmt.Errorf("%s must be an absolute HTTP(S) URL", name)
	}
	if u.User != nil {
		return fmt.Errorf("%s must not contain URL credentials", name)
	}
	if u.Scheme == "https" {
		return nil
	}
	host := strings.ToLower(u.Hostname())
	ip := net.ParseIP(host)
	if host != "localhost" && (ip == nil || !ip.IsLoopback()) {
		return fmt.Errorf("%s must use HTTPS or an HTTP loopback address", name)
	}
	return nil
}

func reservedOIDCAuthParameter(key string) bool {
	switch strings.ToLower(key) {
	case "response_type", "client_id", "redirect_uri", "scope", "state", "nonce", "code_challenge", "code_challenge_method", "audience", "resource":
		return true
	default:
		return false
	}
}

func ValidateProfile(p Profile) error {
	if strings.TrimSpace(p.Name) == "" {
		return errors.New("profile name is required")
	}
	if strings.ContainsAny(p.Name, "\r\n\t/") {
		return fmt.Errorf("invalid profile name %q", p.Name)
	}
	if strings.TrimSpace(p.Provider) == "" {
		return errors.New("profile provider is required")
	}
	if strings.TrimSpace(p.Username) == "" {
		return errors.New("profile username is required")
	}
	if (strings.TrimSpace(p.S3.AccessKeyID) == "") != (strings.TrimSpace(p.S3.SecretAccessKey) == "") {
		return errors.New("S3 access key and secret key must be supplied together")
	}
	return nil
}

func (p Provider) Redacted() Provider {
	if p.OIDC != nil {
		c := *p.OIDC
		if c.ClientSecret != "" {
			c.ClientSecret = "[redacted]"
		}
		p.OIDC = &c
	}
	return p
}

func (p Profile) Redacted() Profile {
	if p.MCPKey != "" {
		p.MCPKey = "[redacted]"
	}
	if p.BrokerKey != "" {
		p.BrokerKey = "[redacted]"
	}
	if p.EmbeddingAPIKey != "" {
		p.EmbeddingAPIKey = "[redacted]"
	}
	if p.RerankAPIKey != "" {
		p.RerankAPIKey = "[redacted]"
	}
	if p.OIDC != nil {
		s := *p.OIDC
		if s.AccessToken != "" {
			s.AccessToken = "[redacted]"
		}
		if s.RefreshToken != "" {
			s.RefreshToken = "[redacted]"
		}
		if s.IDToken != "" {
			s.IDToken = "[redacted]"
		}
		p.OIDC = &s
	}
	if p.S3.AccessKeyID != "" {
		p.S3.AccessKeyID = "[redacted]"
	}
	if p.S3.SecretAccessKey != "" {
		p.S3.SecretAccessKey = "[redacted]"
	}
	if p.S3.SessionToken != "" {
		p.S3.SessionToken = "[redacted]"
	}
	return p
}
