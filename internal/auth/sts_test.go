package auth

import (
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"net/url"
	"strings"
	"testing"
	"time"
)

func TestAWSSTSExchangerUsesOIDCTokenAndReturnsCompleteTemporaryTopology(t *testing.T) {
	expires := time.Now().UTC().Add(time.Hour).Truncate(time.Second)
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		body, _ := io.ReadAll(r.Body)
		values, err := url.ParseQuery(string(body))
		if err != nil {
			t.Error(err)
		}
		for key, want := range map[string]string{"Action": "AssumeRoleWithWebIdentity", "RoleArn": "arn:aws:iam::123456789012:role/graphit", "WebIdentityToken": "id-token", "DurationSeconds": "1800"} {
			if values.Get(key) != want {
				t.Errorf("STS %s = %q, want %q", key, values.Get(key), want)
			}
		}
		if !strings.HasPrefix(values.Get("RoleSessionName"), "graphit-test-") {
			t.Errorf("STS session name = %q", values.Get("RoleSessionName"))
		}
		var policy stsPolicyDocument
		if err := json.Unmarshal([]byte(values.Get("Policy")), &policy); err != nil {
			t.Error(err)
		} else if len(policy.Statement) != 4 || len(policy.Statement[2].Resource) != 1 || policy.Statement[2].Resource[0] != "arn:aws:s3:::artifacts/graphit/v2/projects/project-a/*" {
			t.Errorf("project session policy = %#v", policy)
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
	credentials, err := (AWSSTSExchanger{}).ExchangeForScope(t.Context(), provider, Profile{Name: "alice", Username: "alice", OIDC: &OIDCSession{IDToken: "id-token", AccessToken: "access-token"}}, ProjectStorageScope("project-a"))
	if err != nil {
		t.Fatal(err)
	}
	if credentials.AccessKeyID != "temporary-access" || credentials.SecretAccessKey != "temporary-secret" || credentials.SessionToken != "temporary-token" || !credentials.ExpiresAt.Equal(expires) || credentials.Bucket != "artifacts" || credentials.Region != "us-east-1" || credentials.Endpoint != "https://s3.example" || len(credentials.Prefixes) != 1 || credentials.Prefixes[0] != "graphit" {
		t.Fatalf("credentials=%#v", credentials.RedactedForTest())
	}
}

func TestAWSSTSExchangerCanUseAccessTokenAndRejectsMissingSession(t *testing.T) {
	provider := Provider{Type: ProviderOIDC, S3: S3Config{Bucket: "artifacts", CredentialSource: "sts"}, STS: &STSConfig{RoleARN: "role", UseAccessToken: true}}
	if _, err := (AWSSTSExchanger{}).ExchangeForScope(t.Context(), provider, Profile{}, ProjectStorageScope("project-a")); err == nil || !strings.Contains(err.Error(), "OIDC session") {
		t.Fatalf("missing session error=%v", err)
	}
	if _, err := (AWSSTSExchanger{}).ExchangeForScope(t.Context(), provider, Profile{OIDC: &OIDCSession{IDToken: "id-only"}}, ProjectStorageScope("project-a")); err == nil || !strings.Contains(err.Error(), "token is missing") {
		t.Fatalf("missing access token error=%v", err)
	}
}

func TestSTSSessionPolicyLimitsProjectUserAndHubNamespaces(t *testing.T) {
	provider := Provider{Type: ProviderOIDC, S3: S3Config{Bucket: "artifacts", Prefix: "tenant", CredentialSource: "sts"}, STS: &STSConfig{RoleARN: "role"}}
	profile := Profile{Username: "alice"}
	tests := []struct {
		name  string
		scope BrokerStorageScope
		want  string
		deny  string
	}{
		{"project", ProjectStorageScope("project-a"), "tenant/v2/projects/project-a/*", "project-b"},
		{"user", UserStorageScope(), "tenant/v2/users/alice/memory/*", "users/bob"},
		{"hub", HubStorageScope(), "tenant/v2/registry/*", "projects/project-a/artifacts"},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			text, err := stsSessionPolicy(provider, profile, tc.scope)
			if err != nil {
				t.Fatal(err)
			}
			if len(text) > 2048 || !strings.Contains(text, tc.want) || strings.Contains(text, tc.deny) {
				t.Fatalf("scope policy does not isolate %s: %s", tc.name, text)
			}
			var policy stsPolicyDocument
			if err := json.Unmarshal([]byte(text), &policy); err != nil {
				t.Fatal(err)
			}
			if len(policy.Statement) != 4 || policy.Statement[1].Condition["StringLike"]["s3:prefix"][0] == "" {
				t.Fatalf("ListBucket is not prefix-limited: %#v", policy)
			}
		})
	}
	userText, err := stsSessionPolicy(provider, profile, UserStorageScope())
	if err != nil {
		t.Fatal(err)
	}
	var userPolicy stsPolicyDocument
	if err := json.Unmarshal([]byte(userText), &userPolicy); err != nil {
		t.Fatal(err)
	}
	for _, resource := range userPolicy.Statement[3].Resource {
		if strings.Contains(resource, "projects.json") {
			t.Fatalf("user STS could modify its grant document: %s", resource)
		}
	}
	if _, err := stsSessionPolicy(provider, Profile{Username: "../bob"}, UserStorageScope()); err == nil {
		t.Fatal("unsafe user ID was accepted")
	}
	if _, err := stsSessionPolicy(provider, profile, ProjectStorageScope("../other")); err == nil {
		t.Fatal("unsafe project ID was accepted")
	}
}
