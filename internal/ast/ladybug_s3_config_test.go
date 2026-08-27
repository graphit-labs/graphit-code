package ast

import (
	"testing"

	"github.com/graphit-labs/graphit-code/internal/config"
)

func TestLadybugS3CredentialsPreferConfigAndFallBackToAWS(t *testing.T) {
	t.Setenv("AWS_ACCESS_KEY_ID", "environment-access")
	t.Setenv("AWS_SECRET_ACCESS_KEY", "environment-secret")
	t.Setenv("AWS_SESSION_TOKEN", "environment-token")

	configured := resolvedLadybugS3Credentials(config.S3Config{
		AccessKeyID:     "configured-access",
		SecretAccessKey: "configured-secret",
		Endpoint:        "http://localhost:9000/",
	})
	if configured.AccessKeyID != "configured-access" || configured.SecretAccessKey != "configured-secret" {
		t.Fatalf("configured credentials = %#v", configured)
	}
	if configured.SessionToken != "" {
		t.Fatalf("configured credentials inherited environment session token %q", configured.SessionToken)
	}
	if configured.Endpoint != "localhost:9000" || !configured.DisableSSL || !configured.PathStyle {
		t.Fatalf("configured endpoint = %#v; want normalized HTTP path-style endpoint", configured)
	}

	fallback := resolvedLadybugS3Credentials(config.S3Config{})
	if fallback.AccessKeyID != "environment-access" || fallback.SecretAccessKey != "environment-secret" {
		t.Fatalf("fallback credentials = %#v", fallback)
	}
	if fallback.SessionToken != "environment-token" {
		t.Fatalf("fallback session token = %q; want environment-token", fallback.SessionToken)
	}
}
