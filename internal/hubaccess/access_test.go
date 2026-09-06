package hubaccess

import (
	"context"
	"errors"
	"sync"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/s3store"
)

const (
	projectOne = "01ARZ3NDEKTSV4RRFFQ69G5FAV"
	projectTwo = "01BX5ZZKBKACTAV9WEVGEMMVRZ"
)

type grantReader struct {
	mu       sync.Mutex
	values   map[string][]byte
	errors   map[string]error
	requests map[string]int
}

func (r *grantReader) Get(_ context.Context, key string) ([]byte, error) {
	r.mu.Lock()
	defer r.mu.Unlock()
	if r.requests == nil {
		r.requests = make(map[string]int)
	}
	r.requests[key]++
	if err := r.errors[key]; err != nil {
		return nil, err
	}
	value, ok := r.values[key]
	if !ok {
		return nil, s3store.ErrNotFound
	}
	return value, nil
}

func TestResolveReadsNPlusTwoDocumentsAndUnionsSelectors(t *testing.T) {
	reader := &grantReader{values: map[string][]byte{
		GlobalProjectsKey():         []byte(`{"v":1,"projects":[{"id":"` + projectOne + `"}]}`),
		UserProjectsKey("alice"):    []byte(`{"v":1,"projects":[{"name_prefix":"payments-"}]}`),
		TeamProjectsKey("platform"): []byte(`{"v":1,"projects":[{"all":true}]}`),
	}}
	grants, err := Resolve(context.Background(), reader, Subject{UserID: "alice", TeamIDs: []string{"platform"}})
	if err != nil {
		t.Fatal(err)
	}
	if !grants.Allows(projectOne, "anything") || !grants.Allows(projectTwo, "payments-api") || !grants.All() {
		t.Fatal("union did not preserve exact, prefix, and all selectors")
	}
	if got := len(reader.requests); got != 3 {
		t.Fatalf("read %d distinct documents, want n+2 = 3", got)
	}
	for key, count := range reader.requests {
		if count != 1 {
			t.Fatalf("read %s %d times, want once", key, count)
		}
	}
}

func TestResolveMissingDocumentsGrantNothing(t *testing.T) {
	grants, err := Resolve(context.Background(), &grantReader{}, Subject{UserID: "alice"})
	if err != nil {
		t.Fatal(err)
	}
	if grants.Allows(projectOne, "payments-api") {
		t.Fatal("missing documents granted access")
	}
}

func TestResolveFailsClosedForMalformedOrUnavailableDocument(t *testing.T) {
	for name, reader := range map[string]*grantReader{
		"malformed": {values: map[string][]byte{GlobalProjectsKey(): []byte(`{"v":1,"projects":[{"all":true,"id":"` + projectOne + `"}]}`)}},
		"trailing":  {values: map[string][]byte{GlobalProjectsKey(): []byte(`{"v":1,"projects":[]} {"v":1,"projects":[]}`)}},
		"backend":   {errors: map[string]error{UserProjectsKey("alice"): errors.New("denied")}},
	} {
		t.Run(name, func(t *testing.T) {
			if _, err := Resolve(context.Background(), reader, Subject{UserID: "alice"}); err == nil {
				t.Fatal("Resolve succeeded")
			}
		})
	}
}

func TestTrustedSubjectIsRequiredAndTeamIDsAreCanonical(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)
	t.Setenv("GRAPHIT_HUB_SUBJECT_USER", "")
	t.Setenv("GRAPHIT_HUB_SUBJECT_TEAMS", "")
	if _, err := TrustedSubject(context.Background()); !errors.Is(err, ErrUnauthenticated) {
		t.Fatalf("TrustedSubject error = %v", err)
	}
	ctx, err := WithTrustedSubject(context.Background(), Subject{UserID: "alice", TeamIDs: []string{"z", "a", "z"}})
	if err != nil {
		t.Fatal(err)
	}
	subject, err := TrustedSubject(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if len(subject.TeamIDs) != 2 || subject.TeamIDs[0] != "a" || subject.TeamIDs[1] != "z" {
		t.Fatalf("teams = %#v", subject.TeamIDs)
	}
}

func TestTrustedSubjectUsesOnlyGlobalDeploymentConfigurationAsFallback(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)
	t.Setenv("GRAPHIT_HUB_SUBJECT_USER", "alice")
	t.Setenv("GRAPHIT_HUB_SUBJECT_TEAMS", "platform, security;platform")
	subject, err := TrustedSubject(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if subject.UserID != "alice" || len(subject.TeamIDs) != 2 || subject.TeamIDs[0] != "platform" || subject.TeamIDs[1] != "security" {
		t.Fatalf("subject = %#v", subject)
	}
}
