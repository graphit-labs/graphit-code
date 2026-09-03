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

	// Writable states that this caller intends to WRITE to a remote store, and it is read for
	// a remote URI only — a local store is always writable.
	//
	// WHY THIS IS AN INTENT RATHER THAN A DERIVED FACT. A remote store used to be read-only by
	// construction: the guards on every mutating method tested the URI's scheme, so nothing
	// could write to `s3://` at all. That is the right default and it is kept — a consumer of a
	// published Hub artifact never sets this field, so it cannot fork the version the registry
	// names, which is the one thing the layout must not allow.
	//
	// What the scheme could not express is the case where writing remotely is the POINT: a
	// memory scope whose table lives in the bucket and is extended by every unit that shares
	// those memories. Deriving read-only from the scheme conflated "this is object storage"
	// with "this is somebody else's artifact", and only the second is about permission.
	//
	// A caller that sets it accepts what comes with concurrent writers: a commit can lose the
	// race and has to be retried. See withCommitRetry.
	Writable bool

	// StrongReadConsistency makes every read refresh the table manifest first.
	// Coordination data must enable it: a cached snapshot is acceptable for a
	// search index, but never for deciding whether a shared lease is free.
	StrongReadConsistency bool
}

// ReadOnly reports whether this configuration refuses writes.
//
// Remote and read-only are DIFFERENT questions, which is why there are two methods. `IsRemote`
// answers "does this live in object storage", which decides how it is addressed and whether
// maintenance applies; this answers "may this caller write", which is a statement the caller
// makes.
func (c Config) ReadOnly() bool { return c.IsRemote() && !c.Writable }

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
