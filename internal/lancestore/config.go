package lancestore

import (
	"fmt"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/config"
)

// object_store configuration keys.
//
// Spelled out here rather than aliased from the vendor package so this file carries no build
// tag: the configuration surface has to type-check in a build without the native library. They
// are the object_store crate's own config names and are stable — store_lancedb.go asserts they
// still match the vendor constants, so a rename upstream fails the build instead of silently
// dropping an option.
const (
	storageKeyRegion        = "region"
	storageKeyEndpoint      = "endpoint"
	storageKeyAccessKeyID   = "access_key_id"
	storageKeySecretKey     = "secret_access_key"
	storageKeyVirtualHosted = "virtual_hosted_style_request"
	storageKeyAllowHTTP     = "allow_http"
)

// Config says where a store lives and, when it is remote, how to reach it.
type Config struct {
	// URI is either a local directory path or an `s3://bucket/prefix` location. The scheme is
	// what decides the mode, so a caller never has to say which it means.
	URI string

	// S3 carries the bucket's region, endpoint, addressing and optional explicit credentials.
	// It is IGNORED for a local URI.
	S3 config.S3Config
}

// IsRemote reports whether the URI names object storage rather than a directory.
func (c Config) IsRemote() bool { return isRemoteURI(c.URI) }

func isRemoteURI(uri string) bool {
	u := strings.ToLower(strings.TrimSpace(uri))
	for _, scheme := range []string{"s3://", "gs://", "gcs://", "az://", "azure://"} {
		if strings.HasPrefix(u, scheme) {
			return true
		}
	}
	return false
}

// Validate refuses a configuration that cannot open.
func (c Config) Validate() error {
	if strings.TrimSpace(c.URI) == "" {
		return fmt.Errorf("lancestore: no URI")
	}
	if c.IsRemote() && c.S3.Region == "" && c.S3.Endpoint == "" {
		// A region or an endpoint is needed to address the bucket. Neither means the AWS
		// default chain has nothing to work from, and the failure would otherwise arrive as a
		// timeout on the first query rather than here.
		return fmt.Errorf("lancestore: %s needs a region or an endpoint", c.URI)
	}
	return nil
}

// storageOptions renders the object_store settings LanceDB needs for a remote URI.
//
// The two derived settings are the ones an S3-compatible server needs, and they are derived the
// same way internal/s3store derives them, so a bucket that works for the Hub works here:
//
//   - a custom endpoint implies PATH-STYLE addressing, because MinIO and most compatible
//     servers do not serve virtual-host style buckets;
//   - an `http://` endpoint has to be allowed explicitly, or object_store refuses it.
func (c Config) storageOptions() map[string]string {
	if !c.IsRemote() {
		return nil
	}
	opts := map[string]string{}
	if c.S3.Region != "" {
		opts[storageKeyRegion] = c.S3.Region
	}
	if c.S3.Endpoint != "" {
		opts[storageKeyEndpoint] = c.S3.Endpoint
		opts[storageKeyVirtualHosted] = "false"
		if strings.HasPrefix(strings.ToLower(c.S3.Endpoint), "http://") {
			opts[storageKeyAllowHTTP] = "true"
		}
	}
	if c.S3.HasStaticCredentials() {
		opts[storageKeyAccessKeyID] = c.S3.AccessKeyID
		opts[storageKeySecretKey] = c.S3.SecretAccessKey
	}
	return opts
}
