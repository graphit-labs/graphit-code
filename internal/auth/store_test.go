package auth

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func TestStoreProviderProfileLifecycleAndPermissions(t *testing.T) {
	path := filepath.Join(t.TempDir(), "global", "auth.json")
	store := OpenAt(path)
	provider := Provider{Name: "office", Type: ProviderLocal, Local: &LocalConfig{}, S3: S3Config{Bucket: "bucket"}}
	if err := store.AddProvider(provider); err != nil {
		t.Fatal(err)
	}
	if err := store.Login(Profile{Name: "alice", Provider: "office", Username: "alice", MCPKey: "mcp-secret", S3: S3Credentials{AccessKeyID: "key", SecretAccessKey: "secret"}}); err != nil {
		t.Fatal(err)
	}

	snapshot, err := store.Active()
	if err != nil {
		t.Fatal(err)
	}
	if snapshot.Profile.Name != "alice" || snapshot.Provider.Revision != 1 {
		t.Fatalf("unexpected snapshot: %#v", snapshot)
	}
	if mode := mustMode(t, filepath.Dir(path)); mode.Perm() != 0o700 {
		t.Fatalf("dir mode = %o", mode.Perm())
	}
	if mode := mustMode(t, path); mode.Perm() != 0o600 {
		t.Fatalf("file mode = %o", mode.Perm())
	}

	state, err := store.Load()
	if err != nil {
		t.Fatal(err)
	}
	redacted := state.Profiles["alice"].Redacted()
	if redacted.MCPKey != "[redacted]" || redacted.S3.SecretAccessKey != "[redacted]" {
		t.Fatalf("secrets were not redacted: %#v", redacted)
	}
	if err := store.Logout(""); err != nil {
		t.Fatal(err)
	}
	if _, err := store.Active(); err == nil {
		t.Fatal("expected no active profile")
	}
}

func TestProviderRevisionInvalidatesProfilesAndCascadeIsExplicit(t *testing.T) {
	store := OpenAt(filepath.Join(t.TempDir(), "auth.json"))
	p := Provider{Name: "corp", Type: ProviderOIDC, OIDC: &OIDCConfig{Issuer: "https://issuer.example", ClientID: "client", UsernameClaim: "preferred_username"}}
	if err := store.AddProvider(p); err != nil {
		t.Fatal(err)
	}
	if err := store.Login(Profile{Name: "me", Provider: "corp", Username: "me", Issuer: p.OIDC.Issuer, Subject: "subject", OIDC: &OIDCSession{AccessToken: "access", IDToken: "id", ExpiresAt: time.Now().Add(time.Hour)}}); err != nil {
		t.Fatal(err)
	}
	p.OIDC.ClientID = "new-client"
	if err := store.UpdateProvider(p); err != nil {
		t.Fatal(err)
	}
	if _, err := store.Active(); err == nil || !strings.Contains(err.Error(), "log in again") {
		t.Fatalf("expected stale-profile error, got %v", err)
	}
	if err := store.RemoveProvider("corp", false); err == nil {
		t.Fatal("expected referenced-provider error")
	}
	if err := store.RemoveProvider("corp", true); err != nil {
		t.Fatal(err)
	}
	state, err := store.Load()
	if err != nil {
		t.Fatal(err)
	}
	if len(state.Providers) != 0 || len(state.Profiles) != 0 || state.ActiveProfile != "" {
		t.Fatalf("cascade left state: %#v", state)
	}
}

func TestValidationRejectsPartialSecrets(t *testing.T) {
	err := ValidateProfile(Profile{Name: "me", Provider: "local", Username: "me", S3: S3Credentials{AccessKeyID: "only"}})
	if err == nil {
		t.Fatal("expected partial S3 credentials to fail")
	}
}

func TestFailedLoginPreservesActiveProfile(t *testing.T) {
	store := OpenAt(filepath.Join(t.TempDir(), "auth.json"))
	if err := store.AddProvider(Provider{Name: "local", Type: ProviderLocal, Local: &LocalConfig{}}); err != nil {
		t.Fatal(err)
	}
	if err := store.Login(Profile{Name: "alice", Provider: "local", Username: "alice"}); err != nil {
		t.Fatal(err)
	}
	if err := store.Login(Profile{Name: "invalid", Provider: "missing", Username: "invalid"}); err == nil {
		t.Fatal("expected failed login")
	}
	snapshot, err := store.Active()
	if err != nil {
		t.Fatal(err)
	}
	if snapshot.Profile.Name != "alice" {
		t.Fatalf("active profile changed after failed login: %#v", snapshot.Profile)
	}
}

func TestEnsureDefaultLocalProviderCreatesCanonicalProviderWithoutProfile(t *testing.T) {
	store := OpenAt(filepath.Join(t.TempDir(), "auth.json"))
	provider, created, err := store.EnsureDefaultLocalProvider()
	if err != nil {
		t.Fatal(err)
	}
	if !created || provider.Name != DefaultLocalProviderName || provider.Type != ProviderLocal || provider.Local == nil {
		t.Fatalf("provider = %#v, created = %t", provider, created)
	}
	for name, service := range map[string]AIServiceConfig{"embedding": provider.AI.Embedding, "rerank": provider.AI.Rerank} {
		if service.Mode != ServiceLocal || service.ONNX == nil || *service.ONNX != DefaultONNXExecutionConfig() {
			t.Fatalf("%s service = %#v", name, service)
		}
	}
	state, err := store.Load()
	if err != nil {
		t.Fatal(err)
	}
	if state.ActiveProfile != "" || len(state.Profiles) != 0 {
		t.Fatalf("default provider created an account profile: %#v", state)
	}

	again, createdAgain, err := store.EnsureDefaultLocalProvider()
	if err != nil {
		t.Fatal(err)
	}
	if createdAgain || again.Revision != provider.Revision || !again.UpdatedAt.Equal(provider.UpdatedAt) {
		t.Fatalf("repeated ensure changed provider: before=%#v after=%#v created=%t", provider, again, createdAgain)
	}
}

func TestEnsureDefaultLocalProviderCompletesOnlyMissingDefaults(t *testing.T) {
	store := OpenAt(filepath.Join(t.TempDir(), "auth.json"))
	cpu := ONNXExecutionConfig{Device: ONNXDeviceCPU, DeviceID: 4}
	partial := Provider{
		Name: DefaultLocalProviderName, Type: ProviderLocal,
		Local: &LocalConfig{AllowDaemonMCPKey: true},
		AI: AIConfig{
			Embedding: AIServiceConfig{Mode: ServiceLocal, ONNX: &cpu},
			Rerank:    AIServiceConfig{},
		},
	}
	if err := store.AddProvider(partial); err != nil {
		t.Fatal(err)
	}
	if err := store.Login(Profile{Name: "existing", Provider: DefaultLocalProviderName, Username: "alice"}); err != nil {
		t.Fatal(err)
	}
	provider, created, err := store.EnsureDefaultLocalProvider()
	if err != nil {
		t.Fatal(err)
	}
	if created || !provider.Local.AllowDaemonMCPKey || provider.AI.Embedding.ONNX == nil || *provider.AI.Embedding.ONNX != cpu {
		t.Fatalf("explicit local configuration was overwritten: %#v", provider)
	}
	if provider.AI.Rerank.Mode != ServiceLocal || provider.AI.Rerank.ONNX == nil || *provider.AI.Rerank.ONNX != DefaultONNXExecutionConfig() {
		t.Fatalf("missing rerank defaults were not completed: %#v", provider.AI.Rerank)
	}
	snapshot, err := store.Active()
	if err != nil {
		t.Fatal(err)
	}
	if snapshot.Profile.Name != "existing" || snapshot.Profile.ProviderRevision != provider.Revision {
		t.Fatalf("active selection was not preserved: %#v", snapshot)
	}
}

func TestActiveOrDefaultLocalRecreatesRemovedProvider(t *testing.T) {
	store := OpenAt(filepath.Join(t.TempDir(), "auth.json"))
	if _, _, err := store.EnsureDefaultLocalProvider(); err != nil {
		t.Fatal(err)
	}
	if err := store.RemoveProvider(DefaultLocalProviderName, false); err != nil {
		t.Fatal(err)
	}
	snapshot, err := store.ActiveOrDefaultLocal()
	if err != nil {
		t.Fatal(err)
	}
	if snapshot.Provider.Name != DefaultLocalProviderName || snapshot.Profile.Name != "" {
		t.Fatalf("snapshot = %#v", snapshot)
	}
	state, err := store.Load()
	if err != nil {
		t.Fatal(err)
	}
	if _, ok := state.Providers[DefaultLocalProviderName]; !ok {
		t.Fatal("default local provider was not persisted again")
	}
}

func TestActiveOrDefaultLocalDoesNotCreateLocalWithActiveProfile(t *testing.T) {
	store := OpenAt(filepath.Join(t.TempDir(), "auth.json"))
	if err := store.AddProvider(Provider{Name: "chosen", Type: ProviderLocal, Local: &LocalConfig{}}); err != nil {
		t.Fatal(err)
	}
	if err := store.Login(Profile{Name: "alice", Provider: "chosen", Username: "alice"}); err != nil {
		t.Fatal(err)
	}
	snapshot, err := store.ActiveOrDefaultLocal()
	if err != nil {
		t.Fatal(err)
	}
	if snapshot.Provider.Name != "chosen" || snapshot.Profile.Name != "alice" {
		t.Fatalf("snapshot = %#v", snapshot)
	}
	state, err := store.Load()
	if err != nil {
		t.Fatal(err)
	}
	if _, ok := state.Providers[DefaultLocalProviderName]; ok {
		t.Fatal("local provider was created despite an active profile")
	}
}

func TestEnsureDefaultLocalProviderRejectsConflictingType(t *testing.T) {
	store := OpenAt(filepath.Join(t.TempDir(), "auth.json"))
	conflict := Provider{Name: DefaultLocalProviderName, Type: ProviderOIDC, OIDC: &OIDCConfig{Issuer: "https://issuer.example", ClientID: "client", UsernameClaim: "preferred_username"}}
	if err := store.AddProvider(conflict); err != nil {
		t.Fatal(err)
	}
	if _, _, err := store.EnsureDefaultLocalProvider(); err == nil || !strings.Contains(err.Error(), "exists with type") {
		t.Fatalf("error = %v", err)
	}
	state, err := store.Load()
	if err != nil {
		t.Fatal(err)
	}
	if state.Providers[DefaultLocalProviderName].Type != ProviderOIDC {
		t.Fatalf("conflicting provider was overwritten: %#v", state.Providers[DefaultLocalProviderName])
	}
}

func mustMode(t *testing.T, path string) os.FileMode {
	t.Helper()
	info, err := os.Stat(path)
	if err != nil {
		t.Fatal(err)
	}
	return info.Mode()
}
