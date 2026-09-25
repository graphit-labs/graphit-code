package commands

import (
	"context"
	"encoding/hex"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"runtime"
	"slices"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/auth"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/daemon"
	"github.com/graphit-labs/graphit-code/internal/hubaccess"
)

func TestDaemonHelpDocumentsWatchConfiguration(t *testing.T) {
	text := newDaemonCmd().Long
	for _, expected := range []string{
		"modules.sync false",
		"GRAPHIT_MODULES_SYNC=false",
		"removes the per-project watcher",
		"explicit graphit sync",
		"verified end-user access token",
		"regenerated on each daemon start",
		"daemon restart                   Restart through the OS manager",
	} {
		if !strings.Contains(text, expected) {
			t.Errorf("daemon help does not contain %q", expected)
		}
	}
}

func TestDaemonMCPMuxHealthAndMCPRoutes(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())

	mcpCalled := false
	handler := newDaemonMCPMux(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		mcpCalled = true
		w.WriteHeader(http.StatusNoContent)
	}), daemonMCPMuxOptions{RuntimeKey: "runtime-key"})

	healthRecorder := httptest.NewRecorder()
	handler.ServeHTTP(healthRecorder, httptest.NewRequest(http.MethodGet, "/health", nil))
	if healthRecorder.Code != http.StatusOK {
		t.Fatalf("health status = %d; want %d", healthRecorder.Code, http.StatusOK)
	}
	if got := strings.TrimSpace(healthRecorder.Body.String()); got != `{"status":"ok"}` {
		t.Fatalf("health body = %q; want ok JSON", got)
	}
	if mcpCalled {
		t.Fatal("health request reached MCP handler")
	}

	unauthorized := httptest.NewRecorder()
	handler.ServeHTTP(unauthorized, httptest.NewRequest(http.MethodPost, "/mcp", nil))
	if unauthorized.Code != http.StatusUnauthorized {
		t.Fatalf("unauthenticated MCP status = %d; want %d", unauthorized.Code, http.StatusUnauthorized)
	}
	if mcpCalled {
		t.Fatal("unauthenticated request reached MCP handler")
	}

	mcpRequest := httptest.NewRequest(http.MethodPost, "/mcp", nil)
	mcpRequest.Header.Set("Authorization", "Bearer runtime-key")
	mcpRecorder := httptest.NewRecorder()
	handler.ServeHTTP(mcpRecorder, mcpRequest)
	if mcpRecorder.Code != http.StatusNoContent || !mcpCalled {
		t.Fatalf("MCP route status = %d, called = %v; want 204 and delegation", mcpRecorder.Code, mcpCalled)
	}
}

func TestDaemonRestartStartsThroughManager(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())

	called := false
	cmd := newDaemonRestartCmdWithEnsure(func() (bool, error) {
		called = true
		return true, nil
	})
	if err := cmd.Execute(); err != nil {
		t.Fatal(err)
	}
	if !called {
		t.Fatal("restart did not invoke the managed daemon starter")
	}
}

func TestDaemonRestartReportsBackgroundStartFailure(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())

	want := errors.New("start failed")
	cmd := newDaemonRestartCmdWithEnsure(func() (bool, error) {
		return false, want
	})
	if err := cmd.Execute(); !errors.Is(err, want) {
		t.Fatalf("restart error = %v, want wrapped %v", err, want)
	}
}

func TestManagedReplacementReturnsFailureForSupervisor(t *testing.T) {
	spawned := false
	err := finishDaemonReplacement(true, func() error { spawned = true; return nil })
	if !errors.Is(err, daemon.ErrReplace) || spawned {
		t.Fatalf("managed replacement err=%v spawned=%t", err, spawned)
	}
	if err := finishDaemonReplacement(false, func() error { spawned = true; return nil }); err != nil || !spawned {
		t.Fatalf("foreground replacement err=%v spawned=%t", err, spawned)
	}
}

func TestManagedDaemonAlwaysServesUI(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	t.Setenv(config.ConfigEnvVar("modules.daemon_ui"), "false")
	if !daemonShouldServeUI(true, false) {
		t.Fatal("managed daemon must serve its UI even when the optional module is disabled")
	}
	if daemonShouldServeUI(false, false) {
		t.Fatal("foreground daemon should keep the configured opt-in behavior")
	}
	if !daemonShouldServeUI(false, true) {
		t.Fatal("--ui should enable the foreground daemon UI")
	}
	t.Setenv(config.ConfigEnvVar("modules.daemon_ui"), "true")
	if !daemonShouldServeUI(false, false) {
		t.Fatal("foreground daemon should honor explicit UI enablement")
	}
}

func TestDaemonFlagsAndServiceCommands(t *testing.T) {
	cmd := newDaemonCmd()
	if cmd.Flags().Lookup("ui") == nil || cmd.Flags().Lookup("ui").Hidden {
		t.Fatal("--ui must be visible for foreground use")
	}
	if flag := cmd.Flags().Lookup("managed"); flag == nil || !flag.Hidden {
		t.Fatal("--managed must remain an internal flag")
	}
	service, _, err := cmd.Find([]string{"service"})
	if err != nil {
		t.Fatal(err)
	}
	for name, want := range map[string]string{"uninstall": "uninstall", "remove": "uninstall", "login": "login"} {
		found, _, err := service.Find([]string{name})
		if err != nil || found == nil || found.Name() != want {
			t.Errorf("service command %q = %v, %v; want %q", name, found, err, want)
		}
	}
	login, _, err := service.Find([]string{"login"})
	if err != nil {
		t.Fatal(err)
	}
	for _, name := range []string{"enable", "status"} {
		found, _, err := login.Find([]string{name})
		if err != nil || found == nil || found.Name() != name {
			t.Errorf("login command %q = %v, %v", name, found, err)
		}
	}
	if found, _, err := login.Find([]string{"disable"}); err == nil && found != nil && found.Name() == "disable" {
		t.Fatal("service login disable must not be registered")
	}
	uninstall, _, err := service.Find([]string{"uninstall"})
	if err != nil || uninstall.Flags().Lookup("only-login") == nil {
		t.Fatal("service uninstall must offer --only-login")
	}
}

func TestBuildDaemonProjectModulesHonorsSyncSwitch(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())

	tests := []struct {
		name     string
		value    string
		wantSync bool
	}{
		{name: "default", wantSync: true},
		{name: "disabled", value: "false", wantSync: false},
		{name: "explicitly enabled", value: "true", wantSync: true},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			projectDir := t.TempDir()
			if tt.value != "" {
				lockfile := `{"config":{"modules":{"sync":"` + tt.value + `"}}}`
				if err := os.WriteFile(filepath.Join(projectDir, brand.LockFileName()), []byte(lockfile), 0o600); err != nil {
					t.Fatal(err)
				}
			}

			cfg := daemon.DefaultConfig()
			cfg.DisableEmbedding = true
			cfg.DisableDream = true
			modules, _, err := buildDaemonProjectModules(projectDir, cfg, nil)
			if err != nil {
				t.Fatal(err)
			}

			gotSync := false
			for _, module := range modules {
				if module.Name() == "sync" {
					gotSync = true
				}
			}
			if gotSync != tt.wantSync {
				t.Fatalf("sync module present=%v, want %v", gotSync, tt.wantSync)
			}
		})
	}
}

func TestBuildDaemonProjectModulesHonorsTaskSwitch(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	for _, tt := range []struct {
		name     string
		disabled bool
	}{
		{name: "enabled"},
		{name: "disabled", disabled: true},
	} {
		t.Run(tt.name, func(t *testing.T) {
			projectDir := t.TempDir()
			value := "true"
			if tt.disabled {
				value = "false"
			}
			lockfile := `{"project":{"id":"project-id"},"config":{"modules":{"task":"` + value + `"}}}`
			if err := os.WriteFile(filepath.Join(projectDir, brand.LockFileName()), []byte(lockfile), 0o600); err != nil {
				t.Fatal(err)
			}
			cfg := daemon.DefaultConfig()
			cfg.DisableEmbedding = true
			cfg.DisableDream = true
			modules, _, err := buildDaemonProjectModules(projectDir, cfg, nil)
			if err != nil {
				t.Fatal(err)
			}
			found := false
			for _, module := range modules {
				found = found || module.Name() == "task_maintenance"
			}
			if found == tt.disabled {
				t.Fatalf("task maintenance present=%v, disabled=%v", found, tt.disabled)
			}
		})
	}
}

func TestResolveDaemonMCPAPIKey(t *testing.T) {
	t.Run("generated runtime keys", func(t *testing.T) {
		t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())

		first, err := resolveDaemonMCPAPIKey()
		if err != nil {
			t.Fatal(err)
		}
		second, err := resolveDaemonMCPAPIKey()
		if err != nil {
			t.Fatal(err)
		}
		if first == second {
			t.Fatal("generated fallback reused a bearer key")
		}
		for _, key := range []string{first, second} {
			decoded, decodeErr := hex.DecodeString(key)
			if decodeErr != nil || len(decoded) != 32 {
				t.Fatalf("generated key %q is not 32 random bytes encoded as hex", key)
			}
		}
	})
}

type daemonVerifierFunc func(context.Context, auth.Provider, string, []string) (auth.VerifiedIdentity, error)

func (f daemonVerifierFunc) VerifyAccessToken(ctx context.Context, provider auth.Provider, token string, audiences []string) (auth.VerifiedIdentity, error) {
	return f(ctx, provider, token, audiences)
}

func TestDaemonBearerAcceptsRuntimeAndBrokerAccessTokens(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	store, err := auth.Open()
	if err != nil {
		t.Fatal(err)
	}
	provider := auth.Provider{
		Name: "broker", Type: auth.ProviderBroker,
		Broker: &auth.BrokerConfig{Endpoint: "https://broker.example", MCPResource: "https://graphit.example.com/mcp"},
		AI: auth.AIConfig{
			Embedding: auth.AIServiceConfig{Mode: auth.ServiceBroker},
			Rerank:    auth.AIServiceConfig{Mode: auth.ServiceBroker},
		},
	}
	if err := store.AddProvider(provider); err != nil {
		t.Fatal(err)
	}
	if err := store.Login(auth.Profile{
		Name: "alice", Provider: provider.Name, Issuer: "https://broker.example", Subject: "subject", Username: "alice",
		OIDC: &auth.OIDCSession{AccessToken: "profile-token", IDToken: "id-token", ExpiresAt: time.Now().Add(-time.Hour)},
	}); err != nil {
		t.Fatal(err)
	}
	verifier := daemonVerifierFunc(func(_ context.Context, gotProvider auth.Provider, token string, audiences []string) (auth.VerifiedIdentity, error) {
		if gotProvider.Type != auth.ProviderBroker || token != "broker-token" {
			return auth.VerifiedIdentity{}, errors.New("invalid token")
		}
		return auth.VerifiedIdentity{Issuer: "https://broker.example", Subject: "caller", Username: "bob", Teams: []string{"platform"}}, nil
	})
	if _, allowed := daemonBearerContextWithVerifier(context.Background(), "runtime-key", "runtime-key", verifier, nil); !allowed {
		t.Fatal("runtime key rejected")
	}
	requestContext, allowed := daemonBearerContextWithVerifier(context.Background(), "broker-token", "runtime-key", verifier, []string{"https://graphit.example.com/mcp"})
	if !allowed || auth.RequestBrokerBearer(requestContext) != "broker-token" {
		t.Fatalf("verified Broker bearer was not bound: allowed=%v token=%q", allowed, auth.RequestBrokerBearer(requestContext))
	}
	subject, err := hubaccess.TrustedSubject(requestContext)
	if err != nil || subject.UserID != "bob" || len(subject.TeamIDs) != 1 || subject.TeamIDs[0] != "platform" {
		t.Fatalf("trusted subject=%#v err=%v", subject, err)
	}
	if _, allowed := daemonBearerContextWithVerifier(context.Background(), "wrong", "runtime-key", verifier, []string{"https://graphit.example.com/mcp"}); allowed {
		t.Fatal("invalid Broker token accepted")
	}
}

func TestConcurrentOIDCMCPRequestsKeepTheirBearerThroughBrokerResolution(t *testing.T) {
	var mu sync.Mutex
	seen := map[string]int{}
	broker := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/.well-known/graphit-broker":
			_ = json.NewEncoder(w).Encode(map[string]any{"version": "1", "services": map[string]any{"hub_access": map[string]any{
				"protocol": "graphit-hub-access-v1", "path": "/v1/hub/access/resolve", "authorization_revision": "9",
			}}})
		case "/v1/hub/access/resolve":
			token := strings.TrimPrefix(r.Header.Get("Authorization"), "Bearer ")
			mu.Lock()
			seen[token]++
			mu.Unlock()
			_ = json.NewEncoder(w).Encode(map[string]any{"v": 1, "authorization_revision": "9", "subject": "issuer|" + token, "selectors": []map[string]any{{"all": true}}})
		default:
			http.NotFound(w, r)
		}
	}))
	defer broker.Close()

	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	store, err := auth.Open()
	if err != nil {
		t.Fatal(err)
	}
	provider := auth.Provider{
		Name: "broker", Type: auth.ProviderBroker,
		Broker: &auth.BrokerConfig{Endpoint: broker.URL, MCPResource: "https://graphit.example.com/mcp"},
		AI:     auth.AIConfig{Embedding: auth.AIServiceConfig{Mode: auth.ServiceBroker}, Rerank: auth.AIServiceConfig{Mode: auth.ServiceBroker}},
	}
	if err := store.AddProvider(provider); err != nil {
		t.Fatal(err)
	}
	if err := store.Login(auth.Profile{Name: "daemon", Provider: provider.Name, Issuer: broker.URL, Subject: "daemon-subject", Username: "daemon", OIDC: &auth.OIDCSession{AccessToken: "profile-token", IDToken: "synthetic-id-token", ExpiresAt: time.Now().Add(time.Hour)}}); err != nil {
		t.Fatal(err)
	}
	verifier := daemonVerifierFunc(func(_ context.Context, _ auth.Provider, token string, audiences []string) (auth.VerifiedIdentity, error) {
		if !slices.Equal(audiences, []string{"https://graphit.example.com/mcp"}) || (token != "alice-token" && token != "bob-token") {
			return auth.VerifiedIdentity{}, errors.New("invalid synthetic token")
		}
		return auth.VerifiedIdentity{Issuer: "issuer", Subject: token, Username: strings.TrimSuffix(token, "-token")}, nil
	})
	client, err := auth.NewBrokerHubAccessClient(context.Background(), broker.Client())
	if err != nil {
		t.Fatal(err)
	}

	var wg sync.WaitGroup
	for _, token := range []string{"alice-token", "bob-token"} {
		token := token
		wg.Add(1)
		go func() {
			defer wg.Done()
			ctx, allowed := daemonBearerContextWithVerifier(context.Background(), token, "runtime-key", verifier, []string{"https://graphit.example.com/mcp"})
			if !allowed {
				t.Errorf("%s was rejected by MCP authentication", token)
				return
			}
			response, resolveErr := client.Resolve(ctx)
			if resolveErr != nil || response.Subject != "issuer|"+token {
				t.Errorf("token=%q response=%#v err=%v", token, response, resolveErr)
			}
		}()
	}
	wg.Wait()
	mu.Lock()
	defer mu.Unlock()
	if seen["alice-token"] != 1 || seen["bob-token"] != 1 || seen["profile-token"] != 0 {
		t.Fatalf("broker bearer crossed MCP requests: %v", seen)
	}
}

func TestWriteDaemonMCPKeyEnforcesPrivateMode(t *testing.T) {
	path := filepath.Join(t.TempDir(), "mcp.key")
	if err := os.WriteFile(path, []byte("old-key"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.Chmod(path, 0o644); err != nil {
		t.Fatal(err)
	}

	if err := writeDaemonMCPKey(path, "active-key"); err != nil {
		t.Fatal(err)
	}
	if !daemonMCPKeyFileMatches(path, "active-key") {
		t.Fatal("written key or file mode does not match the active daemon key")
	}
	if runtime.GOOS == "windows" {
		return
	}
	info, err := os.Stat(path)
	if err != nil {
		t.Fatal(err)
	}
	if got := info.Mode().Perm(); got != 0o600 {
		t.Fatalf("mcp.key mode = %o, want 600", got)
	}
}

func TestSplitLines(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name  string
		input string
		want  int
	}{
		{"empty", "", 0},
		{"single_line_no_newline", "hello", 1},
		{"single_line_with_newline", "hello\n", 1},
		{"two_lines", "hello\nworld", 2},
		{"two_lines_trailing", "hello\nworld\n", 2},
		{"multiple_empty_lines", "\n\n\n", 3},
		{"mixed", "line1\nline2\nline3", 3},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			got := splitLines(tt.input)
			if len(got) != tt.want {
				t.Errorf("splitLines(%q) = %d lines; want %d (lines=%v)", tt.input, len(got), tt.want, got)
			}
		})
	}
}

func TestSplitLastN(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name  string
		input string
		n     int
		want  int
	}{
		{"empty", "", 5, 0},
		{"fewer_than_n", "line1\nline2\n", 5, 2},
		{"exact_n", "line1\nline2\nline3\n", 3, 3},
		{"more_than_n", "line1\nline2\nline3\nline4\nline5\n", 3, 3},
		{"n_is_1", "a\nb\nc\n", 1, 1},
		{"skips_empty_lines", "line1\n\nline2\n\nline3\n", 10, 3},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			got := splitLastN(tt.input, tt.n)
			if len(got) != tt.want {
				t.Errorf("splitLastN(%q, %d) = %d lines; want %d (lines=%v)", tt.input, tt.n, len(got), tt.want, got)
			}
		})
	}

	t.Run("returns_last_entries", func(t *testing.T) {
		t.Parallel()
		got := splitLastN("a\nb\nc\nd\ne\n", 2)
		if len(got) != 2 {
			t.Fatalf("expected 2 lines, got %d", len(got))
		}
		if got[0] != "d" || got[1] != "e" {
			t.Errorf("expected [d e], got %v", got)
		}
	})
}
