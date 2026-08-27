package lancestore

import (
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
		},
	}
	opts := remote.storageOptions()
	if opts[storageKeyAccessKeyID] != "access" || opts[storageKeySecretKey] != "secret" {
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
