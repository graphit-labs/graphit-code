//go:build lancedb

package hub

import (
	"context"
	"encoding/base64"
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/auth"
	"github.com/graphit-labs/graphit-code/internal/brand"
	gitstate "github.com/graphit-labs/graphit-code/internal/git"
	"github.com/graphit-labs/graphit-code/internal/hubaccess"
	"github.com/graphit-labs/graphit-code/internal/lancestore"
	"github.com/graphit-labs/graphit-code/internal/wiki"
	"github.com/oklog/ulid/v2"
)

// TestLiveMinIOBranchHydration is opt-in because it uses the configured development
// MinIO. It publishes only temporary branch artifacts and removes them at exit.
func TestLiveMinIOBranchHydration(t *testing.T) {
	endpoint := os.Getenv("GRAPHIT_LIVE_MINIO_ENDPOINT")
	if endpoint == "" {
		t.Skip("set GRAPHIT_LIVE_MINIO_ENDPOINT to run against development MinIO")
	}
	projectID := os.Getenv("GRAPHIT_LIVE_PROJECT_ID")
	repo := os.Getenv("GRAPHIT_LIVE_PROJECT_REPO")
	tokenFile := os.Getenv("GRAPHIT_LIVE_BROKER_TOKEN_FILE")
	brokerEndpoint := os.Getenv("GRAPHIT_LIVE_BROKER_ENDPOINT")
	if hubaccess.ValidateProjectID(projectID) != nil || repo == "" || tokenFile == "" || brokerEndpoint == "" {
		t.Fatal("live MinIO test needs project ID, repository, broker endpoint and OIDC token file")
	}
	ctx := context.Background()
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), filepath.Join(t.TempDir(), "publisher-global"))
	tokenJSON, err := os.ReadFile(tokenFile)
	if err != nil {
		t.Fatal(err)
	}
	var session struct {
		AccessToken  string `json:"access_token"`
		RefreshToken string `json:"refresh_token"`
		IDToken      string `json:"id_token"`
	}
	if err := json.Unmarshal(tokenJSON, &session); err != nil {
		t.Fatal(err)
	}
	parts := strings.Split(session.AccessToken, ".")
	if len(parts) != 3 || session.IDToken == "" {
		t.Fatal("test file has no complete OIDC token session")
	}
	claimsJSON, err := base64.RawURLEncoding.DecodeString(parts[1])
	if err != nil {
		t.Fatal(err)
	}
	var claims struct {
		Issuer  string `json:"iss"`
		Subject string `json:"sub"`
		Expiry  int64  `json:"exp"`
	}
	if err := json.Unmarshal(claimsJSON, &claims); err != nil {
		t.Fatal(err)
	}
	account, err := auth.Open()
	if err != nil {
		t.Fatal(err)
	}
	if err := account.AddProvider(auth.Provider{Name: "broker-e2e", Type: auth.ProviderBroker,
		Broker: &auth.BrokerConfig{Endpoint: brokerEndpoint},
		AI:     auth.AIConfig{Embedding: auth.AIServiceConfig{Mode: auth.ServiceBroker}, Rerank: auth.AIServiceConfig{Mode: auth.ServiceBroker}},
		S3:     auth.S3Config{CredentialSource: "broker"}}); err != nil {
		t.Fatal(err)
	}
	if err := account.Login(auth.Profile{Name: "broker-e2e", Provider: "broker-e2e", Username: "admin",
		Issuer: claims.Issuer, Subject: claims.Subject, OIDC: &auth.OIDCSession{
			AccessToken: session.AccessToken, RefreshToken: session.RefreshToken, IDToken: session.IDToken,
			TokenType: "Bearer", ExpiresAt: time.Unix(claims.Expiry, 0),
		}}); err != nil {
		t.Fatal(err)
	}
	remote, err := NewS3Store(ctx, nil, nil)
	if err != nil {
		t.Fatal(err)
	}
	if published, err := authorizeHydrationProject(ctx, remote, projectID); err != nil || !published {
		t.Fatalf("Graphit project metadata unavailable: published=%v err=%v", published, err)
	}

	suffix := strings.ToLower(ulid.Make().String())
	branch := "e2e-shallow-" + suffix
	targets := []struct {
		kind ArtifactType
		id   string
		part string
	}{
		{TypeAST, "graphit-e2e-shallow-ast-" + suffix, ast.SearchBundleDir},
		{TypeKnowledge, "graphit-e2e-shallow-knowledge-" + suffix, wiki.WikiIndexDirName},
	}
	registry := registryForStore(ctx, remote)
	baseVersions := make(map[ArtifactType]uint64)
	for _, target := range targets {
		target := target
		t.Cleanup(func() {
			_ = remote.DeleteArtifact(context.Background(), target.kind, target.id, "branch/"+branch, projectID)
			_ = remote.RemoveFile(context.Background(), hubaccess.ProjectRegistryKey(projectID, string(target.kind), target.id))
		})
	}
	source := filepath.Join(t.TempDir(), "graphit-code")
	cmd := exec.Command("git", "clone", "--quiet", "--no-hardlinks", repo, source)
	if output, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("clone Graphit Code: %v: %s", err, output)
	}
	runHydrationGit(t, source, "checkout", "-q", "-b", branch)
	lock := Lockfile{Project: ProjectIdentity{ID: projectID, Name: "graphit-labs-graphit-code"},
		Artifacts: map[ArtifactType]map[string]*LockfileArtifactMeta{}}
	for _, target := range targets {
		lock.Artifacts[target.kind] = map[string]*LockfileArtifactMeta{
			target.id: {Version: "branch/" + branch, RemoteID: target.id, ProjectID: projectID},
		}
	}
	data, err := json.Marshal(lock)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(source, brand.LockFileName()), data, 0o644); err != nil {
		t.Fatal(err)
	}
	runHydrationGit(t, source, "add", brand.LockFileName())
	runHydrationGit(t, source, "-c", "user.name=Graphit Test", "-c", "user.email=test@example.invalid", "commit", "-qm", "e2e shallow base")
	snapshot, err := gitstate.InspectSnapshot(source)
	if err != nil || snapshot.Dirty {
		t.Fatalf("clean test branch: %#v %v", snapshot, err)
	}

	for _, target := range targets {
		stage := t.TempDir()
		local, err := lancestore.Open(ctx, remote.lanceConfig(filepath.Join(stage, target.part), true))
		if err != nil {
			t.Fatal(err)
		}
		table, err := local.CreateTable(ctx, "meta", lancestore.Schema{Fields: []lancestore.Field{
			{Name: "key", Type: lancestore.FieldString}, {Name: "value", Type: lancestore.FieldString},
		}})
		if err != nil {
			t.Fatal(err)
		}
		if err := table.Append(ctx, []lancestore.Row{{"key": "commit", "value": snapshot.Commit}}); err != nil {
			t.Fatal(err)
		}
		if err := local.Close(); err != nil {
			t.Fatal(err)
		}
		history, err := registry.publishBranchLance(ctx, target.id, "branch/"+branch,
			&Entry{ID: target.id, Type: target.kind, ProjectID: projectID}, stage, snapshot)
		if err != nil {
			t.Fatal(err)
		}
		baseVersions[target.kind] = history.Commits[0].Tables["meta"].Version
		if err := remote.writeBranchHistory(ctx, target.kind, target.id, "branch/"+branch, projectID, history); err != nil {
			t.Fatal(err)
		}
		entry := entryFile{Version: hubManifestVersion, Entry: Entry{ID: target.id, Type: target.kind,
			ProjectID: projectID, Versions: []string{"branch/" + branch}}}
		blob, err := json.Marshal(entry)
		if err != nil {
			t.Fatal(err)
		}
		if err := remote.WriteFile(ctx, hubaccess.ProjectRegistryKey(projectID, string(target.kind), target.id), blob); err != nil {
			t.Fatal(err)
		}
	}

	clone := filepath.Join(t.TempDir(), "downloaded")
	cmd = exec.Command("git", "clone", "--quiet", "--no-hardlinks", source, clone)
	if output, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("clone published commit: %v: %s", err, output)
	}
	if result, err := HydrateProjectLanceWithResult(ctx, clone, nil); err != nil || result.ASTBaseCommit != snapshot.Commit || result.KnowledgeBaseCommit != snapshot.Commit {
		t.Fatalf("live MinIO hydration = %#v, %v", result, err)
	}
	for _, target := range targets {
		assertHydrationRows(t, ctx, remote, hydrationTargetPath(clone, target.kind), map[string]string{"commit": snapshot.Commit})
	}

	// Advance the isolated Git branch. Only AST changes; Knowledge points its
	// new commit tag at the unchanged table version rather than rewriting it.
	if err := os.WriteFile(filepath.Join(source, "e2e-shallow-change.txt"), []byte("new commit\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	runHydrationGit(t, source, "add", "e2e-shallow-change.txt")
	runHydrationGit(t, source, "-c", "user.name=Graphit Test", "-c", "user.email=test@example.invalid", "commit", "-qm", "e2e shallow delta")
	next, err := gitstate.InspectSnapshot(source)
	if err != nil || next.Dirty {
		t.Fatalf("clean delta commit: %#v %v", next, err)
	}
	for _, target := range targets {
		value := snapshot.Commit
		if target.kind == TypeAST {
			value = next.Commit
		}
		stage := t.TempDir()
		local, err := lancestore.Open(ctx, remote.lanceConfig(filepath.Join(stage, target.part), true))
		if err != nil {
			t.Fatal(err)
		}
		table, err := local.CreateTable(ctx, "meta", lancestore.Schema{Fields: []lancestore.Field{
			{Name: "key", Type: lancestore.FieldString}, {Name: "value", Type: lancestore.FieldString},
		}})
		if err != nil {
			t.Fatal(err)
		}
		if err := table.Append(ctx, []lancestore.Row{{"key": "commit", "value": value}}); err != nil {
			t.Fatal(err)
		}
		if err := local.Close(); err != nil {
			t.Fatal(err)
		}
		history, err := registry.publishBranchLance(ctx, target.id, "branch/"+branch,
			&Entry{ID: target.id, Type: target.kind, ProjectID: projectID}, stage, next)
		if err != nil {
			t.Fatal(err)
		}
		version := history.Commits[0].Tables["meta"].Version
		if target.kind == TypeAST && version <= baseVersions[target.kind] {
			t.Fatalf("AST delta did not advance its table: %d <= %d", version, baseVersions[target.kind])
		}
		if target.kind == TypeKnowledge && version != baseVersions[target.kind] {
			t.Fatalf("unchanged Knowledge table was rewritten: %d != %d", version, baseVersions[target.kind])
		}
		if err := remote.writeBranchHistory(ctx, target.kind, target.id, "branch/"+branch, projectID, history); err != nil {
			t.Fatal(err)
		}
	}
	runHydrationGit(t, clone, "pull", "--quiet", "--ff-only")
	if result, err := HydrateProjectLanceWithResult(ctx, clone, nil); err != nil || result.ASTBaseCommit != next.Commit || result.KnowledgeBaseCommit != next.Commit {
		t.Fatalf("live MinIO delta hydration = %#v, %v", result, err)
	}
	for _, target := range targets {
		value := snapshot.Commit
		if target.kind == TypeAST {
			value = next.Commit
		}
		assertHydrationRows(t, ctx, remote, hydrationTargetPath(clone, target.kind), map[string]string{"commit": value})
	}
}
