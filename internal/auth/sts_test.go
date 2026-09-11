package auth

import (
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
)

func TestAWSSTSExchangerUsesOIDCTokenAndReturnsCompleteTemporaryTopology(t *testing.T) {
	expires := time.Now().UTC().Add(time.Hour).Truncate(time.Second)
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		body, _ := io.ReadAll(r.Body)
		values := string(body)
		for _, expected := range []string{"Action=AssumeRoleWithWebIdentity", "RoleArn=arn%3Aaws%3Aiam%3A%3A123456789012%3Arole%2Fgraphit", "RoleSessionName=graphit-test", "WebIdentityToken=id-token", "DurationSeconds=1800"} {
			if !strings.Contains(values, expected) {
				t.Errorf("STS body omitted %q: %s", expected, values)
			}
		}
		w.Header().Set("Content-Type", "text/xml")
		_, _ = io.WriteString(w, `<AssumeRoleWithWebIdentityResponse xmlns="https://sts.amazonaws.com/doc/2011-06-15/"><AssumeRoleWithWebIdentityResult><Credentials><AccessKeyId>temporary-access</AccessKeyId><SecretAccessKey>temporary-secret</SecretAccessKey><SessionToken>temporary-token</SessionToken><Expiration>`+expires.Format(time.RFC3339)+`</Expiration></Credentials></AssumeRoleWithWebIdentityResult><ResponseMetadata><RequestId>request</RequestId></ResponseMetadata></AssumeRoleWithWebIdentityResponse>`)
	}))
	defer server.Close()
	provider := Provider{Name: "oidc", Type: ProviderOIDC,
		OIDC: &OIDCConfig{Issuer: "https://id.example", ClientID: "client", UsernameClaim: "preferred_username"},
		S3:   S3Config{Bucket: "artifacts", Region: "us-east-1", Endpoint: "https://s3.example", Prefix: "graphit", CredentialSource: "sts"},
		STS:  &STSConfig{Endpoint: server.URL, RoleARN: "arn:aws:iam::123456789012:role/graphit", RoleSessionName: "graphit-test", DurationSeconds: 1800},
	}
	credentials, err := (AWSSTSExchanger{}).Exchange(t.Context(), provider, Profile{Name: "alice", OIDC: &OIDCSession{IDToken: "id-token", AccessToken: "access-token"}})
	if err != nil {
		t.Fatal(err)
	}
	if credentials.AccessKeyID != "temporary-access" || credentials.SecretAccessKey != "temporary-secret" || credentials.SessionToken != "temporary-token" || !credentials.ExpiresAt.Equal(expires) || credentials.Bucket != "artifacts" || credentials.Region != "us-east-1" || credentials.Endpoint != "https://s3.example" || len(credentials.Prefixes) != 1 || credentials.Prefixes[0] != "graphit" {
		t.Fatalf("credentials=%#v", credentials.RedactedForTest())
	}
}

func TestAWSSTSExchangerCanUseAccessTokenAndRejectsMissingSession(t *testing.T) {
	provider := Provider{STS: &STSConfig{RoleARN: "role", UseAccessToken: true}}
	if _, err := (AWSSTSExchanger{}).Exchange(t.Context(), provider, Profile{}); err == nil || !strings.Contains(err.Error(), "OIDC session") {
		t.Fatalf("missing session error=%v", err)
	}
	if _, err := (AWSSTSExchanger{}).Exchange(t.Context(), provider, Profile{OIDC: &OIDCSession{IDToken: "id-only"}}); err == nil || !strings.Contains(err.Error(), "token is missing") {
		t.Fatalf("missing access token error=%v", err)
	}
}
