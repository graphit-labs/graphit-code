package auth

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"path"
	"regexp"
	"strings"
	"time"

	"github.com/aws/aws-sdk-go-v2/aws"
	awsconfig "github.com/aws/aws-sdk-go-v2/config"
	"github.com/aws/aws-sdk-go-v2/service/sts"
)

type ScopedSTSExchanger interface {
	ExchangeForScope(context.Context, Provider, Profile, BrokerStorageScope) (S3Credentials, error)
}

type AWSSTSExchanger struct{}

func (AWSSTSExchanger) ExchangeForScope(ctx context.Context, provider Provider, profile Profile, scope BrokerStorageScope) (S3Credentials, error) {
	if provider.STS == nil {
		return S3Credentials{}, errors.New("provider has no STS exchange configuration")
	}
	if provider.Type != ProviderOIDC || (provider.S3.CredentialSource != "sts" && provider.S3.CredentialSource != "") {
		return S3Credentials{}, errors.New("scoped STS exchange requires an OIDC STS provider")
	}
	if profile.OIDC == nil {
		return S3Credentials{}, errors.New("STS exchange requires an OIDC session")
	}
	policy, err := stsSessionPolicy(provider, profile, scope)
	if err != nil {
		return S3Credentials{}, err
	}
	token := profile.OIDC.IDToken
	if provider.STS.UseAccessToken {
		token = profile.OIDC.AccessToken
	}
	if token == "" {
		return S3Credentials{}, errors.New("STS exchange token is missing")
	}
	region := firstNonEmpty(provider.S3.Region, "us-east-1")
	cfg, err := awsconfig.LoadDefaultConfig(ctx, awsconfig.WithRegion(region), awsconfig.WithCredentialsProvider(aws.AnonymousCredentials{}))
	if err != nil {
		return S3Credentials{}, fmt.Errorf("configure STS client: %w", err)
	}
	client := sts.NewFromConfig(cfg, func(options *sts.Options) {
		if provider.STS.Endpoint != "" {
			options.BaseEndpoint = aws.String(provider.STS.Endpoint)
		}
	})
	sessionName := scopedSTSSessionName(provider.STS.RoleSessionName, profile.Name, scope)
	input := &sts.AssumeRoleWithWebIdentityInput{RoleArn: aws.String(provider.STS.RoleARN), RoleSessionName: aws.String(sessionName), WebIdentityToken: aws.String(token), Policy: aws.String(policy)}
	if provider.STS.DurationSeconds > 0 {
		input.DurationSeconds = aws.Int32(provider.STS.DurationSeconds)
	}
	output, err := client.AssumeRoleWithWebIdentity(ctx, input)
	if err != nil {
		return S3Credentials{}, fmt.Errorf("STS web identity exchange: %w", err)
	}
	if output.Credentials == nil || output.Credentials.AccessKeyId == nil || output.Credentials.SecretAccessKey == nil || output.Credentials.SessionToken == nil || output.Credentials.Expiration == nil {
		return S3Credentials{}, errors.New("STS response omitted credentials")
	}
	prefixes := []string(nil)
	if prefix := strings.Trim(strings.TrimSpace(provider.S3.Prefix), "/"); prefix != "" {
		prefixes = []string{prefix}
	}
	return S3Credentials{
		AccessKeyID: *output.Credentials.AccessKeyId, SecretAccessKey: *output.Credentials.SecretAccessKey,
		SessionToken: *output.Credentials.SessionToken, ExpiresAt: *output.Credentials.Expiration,
		Bucket: provider.S3.Bucket, Region: region, Endpoint: provider.S3.Endpoint, Prefixes: prefixes,
	}, nil
}

type stsPolicyStatement struct {
	Effect    string                         `json:"Effect"`
	Action    []string                       `json:"Action"`
	Resource  []string                       `json:"Resource"`
	Condition map[string]map[string][]string `json:"Condition,omitempty"`
}

type stsPolicyDocument struct {
	Version   string               `json:"Version"`
	Statement []stsPolicyStatement `json:"Statement"`
}

var stsSubjectPattern = regexp.MustCompile(`^[A-Za-z0-9][A-Za-z0-9._@+-]{0,127}$`)
var stsSessionNameCharacter = regexp.MustCompile(`[^A-Za-z0-9_+=,.@-]`)

// A session policy can only reduce the role's permissions. Each exchange is
// bound to exactly one Graphit namespace, including its ListBucket prefix.
func stsSessionPolicy(provider Provider, profile Profile, scope BrokerStorageScope) (string, error) {
	if err := scope.validate(); err != nil {
		return "", err
	}
	if strings.TrimSpace(provider.S3.Bucket) == "" {
		return "", errors.New("scoped STS requires an S3 bucket")
	}
	root := strings.Trim(strings.TrimSpace(provider.S3.Prefix), "/")
	key := func(parts ...string) string { return path.Join(append([]string{root, "v2"}, parts...)...) }
	var read, write, listed []string
	switch scope.Kind {
	case "project":
		prefix := key("projects", scope.ProjectID)
		read, write, listed = []string{prefix + "/*"}, []string{prefix + "/*"}, []string{prefix + "/*"}
	case "user":
		if !stsSubjectPattern.MatchString(profile.Username) || profile.Username == "anonymous" {
			return "", errors.New("scoped STS requires a safe authenticated user ID")
		}
		prefix := key("users", profile.Username)
		read = []string{prefix + "/projects.json", prefix + "/memory/*"}
		write, listed = []string{prefix + "/memory/*"}, []string{prefix + "/memory/*"}
	case "hub":
		read = []string{key("global") + "/*", key("anonymous") + "/*", key("authenticated") + "/*", key("teams") + "/*", key("registry") + "/*"}
		write = []string{key("registry") + "/*"}
		listed = read
	}
	bucketARN := "arn:aws:s3:::" + provider.S3.Bucket
	objectARNs := func(keys []string) []string {
		out := make([]string, len(keys))
		for i, key := range keys {
			out[i] = bucketARN + "/" + key
		}
		return out
	}
	policy := stsPolicyDocument{Version: "2012-10-17", Statement: []stsPolicyStatement{
		{Effect: "Allow", Action: []string{"s3:GetBucketLocation"}, Resource: []string{bucketARN}},
		{Effect: "Allow", Action: []string{"s3:ListBucket"}, Resource: []string{bucketARN}, Condition: map[string]map[string][]string{"StringLike": {"s3:prefix": listed}}},
		{Effect: "Allow", Action: []string{"s3:GetObject"}, Resource: objectARNs(read)},
		{Effect: "Allow", Action: []string{"s3:PutObject", "s3:DeleteObject"}, Resource: objectARNs(write)},
	}}
	encoded, err := json.Marshal(policy)
	if err != nil {
		return "", err
	}
	if len(encoded) > 2048 {
		return "", errors.New("STS session policy exceeds the 2048-byte limit")
	}
	return string(encoded), nil
}

func scopedSTSSessionName(configured, profileName string, scope BrokerStorageScope) string {
	base := strings.TrimSpace(configured)
	if base == "" {
		base = "graphit-" + profileName
	}
	base = stsSessionNameCharacter.ReplaceAllString(base, "-")
	if len(base) > 54 {
		base = base[:54]
	}
	digest := sha256.Sum256([]byte(scope.cacheKey()))
	return base + "-" + hex.EncodeToString(digest[:4])
}

func usableSTSCredentials(credentials S3Credentials, now time.Time, margin time.Duration) bool {
	return credentials.Complete() && credentials.SessionToken != "" && !credentials.ExpiresAt.IsZero() && credentials.ExpiresAt.After(now.Add(margin))
}
