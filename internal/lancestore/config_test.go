package lancestore

import (
	"context"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/config"
)

func TestStorageOptionsIncludeOnlyACompleteCredentialPair(t *testing.T) {
	remote := Config{
		URI: "s3://bucket/index",
		S3: config.S3Config{
			Region:          "us-east-1",
			AccessKeyID:     "access",
			SecretAccessKey: "secret",
			SessionToken:    "token",
		},
	}
	opts := remote.storageOptions()
	if opts[storageKeyAccessKeyID] != "access" || opts[storageKeySecretKey] != "secret" || opts[storageKeySessionToken] != "token" {
		t.Fatalf("credential options = %#v", opts)
	}

	remote.S3.SecretAccessKey = ""
	opts = remote.storageOptions()
	if _, ok := opts[storageKeyAccessKeyID]; ok {
		t.Fatalf("partial access key reached storage options: %#v", opts)
	}
	if _, ok := opts[storageKeySecretKey]; ok {
		t.Fatalf("partial secret key reached storage options: %#v", opts)
	}
}

func TestConfigRefreshCarriesTemporarySessionTokenWithoutChangingTopology(t *testing.T) {
	cfg := Config{URI: "s3://bucket/index", S3: config.S3Config{Bucket: "bucket", Region: "us-east-1", Prefix: "root", AccessKeyID: "old", SecretAccessKey: "old", SessionToken: "old"}}
	cfg.S3.Refresh = func(context.Context) (config.S3Config, error) {
		refreshed := cfg.S3
		refreshed.AccessKeyID, refreshed.SecretAccessKey, refreshed.SessionToken = "new", "new-secret", "new-token"
		return refreshed, nil
	}
	refreshed, err := cfg.refreshed(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if refreshed.S3.SessionToken != "new-token" || refreshed.storageOptions()[storageKeySessionToken] != "new-token" {
		t.Fatalf("refreshed config lost session token: %#v", refreshed.storageOptions())
	}
}
