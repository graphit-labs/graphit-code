package auth

import (
	"context"
	"errors"
	"fmt"
	"strings"

	"github.com/aws/aws-sdk-go-v2/aws"
	awsconfig "github.com/aws/aws-sdk-go-v2/config"
	"github.com/aws/aws-sdk-go-v2/service/sts"
)

type STSExchanger interface {
	Exchange(context.Context, Provider, Profile) (S3Credentials, error)
}

type AWSSTSExchanger struct{}

func (AWSSTSExchanger) Exchange(ctx context.Context, provider Provider, profile Profile) (S3Credentials, error) {
	if provider.STS == nil {
		return S3Credentials{}, errors.New("provider has no STS exchange configuration")
	}
	if profile.OIDC == nil {
		return S3Credentials{}, errors.New("STS exchange requires an OIDC session")
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
	sessionName := strings.TrimSpace(provider.STS.RoleSessionName)
	if sessionName == "" {
		sessionName = "graphit-" + profile.Name
	}
	input := &sts.AssumeRoleWithWebIdentityInput{RoleArn: aws.String(provider.STS.RoleARN), RoleSessionName: aws.String(sessionName), WebIdentityToken: aws.String(token)}
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
