package hubaccess

import (
	"context"
	"errors"
	"testing"
)

func TestAuthorizeScopedProjectNeedsARegisteredAuthorizer(t *testing.T) {
	RegisterScopedProjectAuthorizer(nil)
	if err := AuthorizeScopedProject(context.Background(), nil, "p1"); !errors.Is(err, ErrScopedAuthorizerUnavailable) {
		t.Fatalf("without an authorizer: %v, want ErrScopedAuthorizerUnavailable", err)
	}

	var gotCfg map[string]any
	var gotID string
	denied := errors.New("denied")
	RegisterScopedProjectAuthorizer(func(_ context.Context, cfg map[string]any, projectID string) error {
		gotCfg, gotID = cfg, projectID
		return denied
	})
	t.Cleanup(func() { RegisterScopedProjectAuthorizer(nil) })

	cfg := map[string]any{"hub": "x"}
	if err := AuthorizeScopedProject(context.Background(), cfg, "p2"); !errors.Is(err, denied) {
		t.Fatalf("registered authorizer result not returned: %v", err)
	}
	if gotID != "p2" || gotCfg["hub"] != "x" {
		t.Fatalf("authorizer received %q %v", gotID, gotCfg)
	}
}
