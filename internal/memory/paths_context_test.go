package memory

import (
	"context"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/hubaccess"
)

func TestUserMemoryScopeUsesVerifiedRequestSubject(t *testing.T) {
	alice, err := hubaccess.WithTrustedSubject(context.Background(), hubaccess.Subject{UserID: "alice"})
	if err != nil {
		t.Fatal(err)
	}
	bob, err := hubaccess.WithTrustedSubject(context.Background(), hubaccess.Subject{UserID: "bob"})
	if err != nil {
		t.Fatal(err)
	}
	if got := resolveScopeIDInContext(alice, "", "user"); got != "alice" {
		t.Fatalf("alice's user scope = %q", got)
	}
	if got := resolveScopeIDInContext(bob, "", "user"); got != "bob" {
		t.Fatalf("bob's user scope = %q", got)
	}
}
