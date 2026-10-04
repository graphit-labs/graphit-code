package hubaccess

import (
	"context"
	"errors"
	"sync"
)

// ErrScopedAuthorizerUnavailable reports a process built without the Hub registry, which
// is the only code able to authorize a project against Broker-scoped storage. Callers
// that see it hand the work to the full CLI.
var ErrScopedAuthorizerUnavailable = errors.New("scoped Hub project authorization is unavailable in this binary")

// ScopedProjectAuthorizer authorizes projectID when storage credentials are scoped by
// the provider (Broker). projectConfig is the project's configuration, or nil.
type ScopedProjectAuthorizer func(ctx context.Context, projectConfig map[string]any, projectID string) error

var (
	scopedMu         sync.RWMutex
	scopedAuthorizer ScopedProjectAuthorizer
)

// RegisterScopedProjectAuthorizer installs the authorizer. The Hub package registers its
// registry so Task and Memory can authorize scoped projects without depending on it.
func RegisterScopedProjectAuthorizer(fn ScopedProjectAuthorizer) {
	scopedMu.Lock()
	defer scopedMu.Unlock()
	scopedAuthorizer = fn
}

// AuthorizeScopedProject authorizes projectID through the registered authorizer.
func AuthorizeScopedProject(ctx context.Context, projectConfig map[string]any, projectID string) error {
	scopedMu.RLock()
	fn := scopedAuthorizer
	scopedMu.RUnlock()
	if fn == nil {
		return ErrScopedAuthorizerUnavailable
	}
	return fn(ctx, projectConfig, projectID)
}
