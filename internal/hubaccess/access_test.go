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

func TestResolveAuthenticatedReadsNPlusThreeDocumentsAndUnionsSelectors(t *testing.T) {
	reader := &grantReader{values: map[string][]byte{
		GlobalProjectsKey():         []byte(`{"v":1,"projects":[{"id":"` + projectOne + `"}]}`),
		AuthenticatedProjectsKey():  []byte(`{"v":1,"projects":[{"all":true}]}`),
		UserProjectsKey("alice"):    []byte(`{"v":1,"projects":[{"name_prefix":"payments-"}]}`),
		TeamProjectsKey("platform"): []byte(`{"v":1,"projects":[]}`),
	}}
	grants, err := Resolve(context.Background(), reader, Subject{UserID: "alice", TeamIDs: []string{"platform"}})
	if err != nil {
		t.Fatal(err)
	}
	if !grants.Allows(projectOne, "anything") || !grants.Allows(projectTwo, "payments-api") || !grants.All() {
		t.Fatal("union did not preserve exact, prefix, and all selectors")
	}
	if got := len(reader.requests); got != 4 {
		t.Fatalf("read %d distinct documents, want n+3 = 4", got)
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

func TestResolveAnonymousReadsOnlyGlobalAndAnonymousDocuments(t *testing.T) {
	reader := &grantReader{values: map[string][]byte{
		AnonymousProjectsKey(): []byte(`{"v":1,"projects":[{"id":"` + projectOne + `"}]}`),
	}}
	grants, err := Resolve(context.Background(), reader, Subject{UserID: AnonymousUserID, TeamIDs: []string{"ignored"}})
	if err != nil {
		t.Fatal(err)
	}
	if !grants.Allows(projectOne, "anything") {
		t.Fatal("anonymous user grant was not applied")
	}
	if len(reader.requests) != 2 || reader.requests[GlobalProjectsKey()] != 1 || reader.requests[AnonymousProjectsKey()] != 1 {
		t.Fatalf("anonymous grant reads = %#v, want global plus anonymous documents", reader.requests)
	}
	if reader.requests[AuthenticatedProjectsKey()] != 0 {
		t.Fatal("anonymous subject read the authenticated grant document")
	}
}

func TestAnonymousIsNotAUserGrantNamespace(t *testing.T) {
	if key := UserProjectsKey(AnonymousUserID); key != "" {
		t.Fatalf("anonymous user grant key = %q, want none", key)
	}
	if key := AnonymousProjectsKey(); key != "v2/anonymous/projects.json" {
		t.Fatalf("anonymous grant key = %q", key)
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

func TestTrustedSubjectDefaultsToAnonymousAndTeamIDsAreCanonical(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)
	t.Setenv("GRAPHIT_HUB_SUBJECT_USER", "")
	t.Setenv("GRAPHIT_HUB_SUBJECT_TEAMS", "")
	subject, err := TrustedSubject(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if subject.UserID != AnonymousUserID || len(subject.TeamIDs) != 0 {
		t.Fatalf("default subject = %#v, want anonymous without teams", subject)
	}
	ctx, err := WithTrustedSubject(context.Background(), Subject{UserID: "alice", TeamIDs: []string{"z", "a", "z"}})
	if err != nil {
		t.Fatal(err)
	}
	subject, err = TrustedSubject(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if len(subject.TeamIDs) != 2 || subject.TeamIDs[0] != "a" || subject.TeamIDs[1] != "z" {
		t.Fatalf("teams = %#v", subject.TeamIDs)
	}
}

func TestAnonymousSubjectNeverCarriesTeamMembership(t *testing.T) {
	ctx, err := WithTrustedSubject(context.Background(), Subject{UserID: AnonymousUserID, TeamIDs: []string{"platform"}})
	if err != nil {
		t.Fatal(err)
	}
	subject, err := TrustedSubject(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if subject.UserID != AnonymousUserID || len(subject.TeamIDs) != 0 {
		t.Fatalf("anonymous subject = %#v, want no teams", subject)
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
