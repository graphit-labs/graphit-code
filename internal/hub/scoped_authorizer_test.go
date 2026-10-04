package hub

import (
	"context"
	"errors"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/hubaccess"
)

// Task and Memory authorize Broker-scoped projects through hubaccess; the registry this
// package installs is what answers them.
func TestHubRegistersTheScopedProjectAuthorizer(t *testing.T) {
	err := hubaccess.AuthorizeScopedProject(context.Background(), nil, "01HZZZZZZZZZZZZZZZZZZZZZZZ")
	if errors.Is(err, hubaccess.ErrScopedAuthorizerUnavailable) {
		t.Fatal("hub did not register its scoped project authorizer")
	}
}
