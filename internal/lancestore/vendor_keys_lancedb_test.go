//go:build lancedb

package lancestore

import (
	"testing"

	"github.com/lancedb/lancedb-go/pkg/contracts"
)

// config.go spells the object_store keys out as literals so it needs no build tag. This is what
// keeps them honest: a rename upstream would otherwise drop a storage option silently, and the
// symptom would be a connection that cannot authenticate — a long way from the cause.
func TestStorageKeysMatchTheVendor(t *testing.T) {
	for _, c := range []struct{ ours, theirs, what string }{
		{storageKeyRegion, contracts.StorageRegion, "region"},
		{storageKeyEndpoint, contracts.StorageEndpoint, "endpoint"},
		{storageKeyVirtualHosted, contracts.StorageVirtualHostedStyleRequest, "virtual hosted style"},
		{storageKeyAllowHTTP, contracts.StorageAllowHTTP, "allow http"},
	} {
		if c.ours != c.theirs {
			t.Errorf("%s key drifted: config.go has %q, the vendor has %q", c.what, c.ours, c.theirs)
		}
	}
}
