package commands

import (
	"bufio"
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/auth"
	"github.com/graphit-labs/graphit-code/internal/brand"
)

func TestBrokerLoginTreatsOnlyMissingS3CapabilityAsLocal(t *testing.T) {
	disabled := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		_ = json.NewEncoder(w).Encode(map[string]any{"version": "1", "services": map[string]any{}})
	}))
	defer disabled.Close()
	provider := auth.Provider{Name: "company", Type: auth.ProviderBroker, Broker: &auth.BrokerConfig{Endpoint: disabled.URL}}
	profile := auth.Profile{Name: "alice", Provider: "company", S3: auth.S3Credentials{AccessKeyID: "old", SecretAccessKey: "old"}}

	err := populateBrokerStorageForLogin(context.Background(), provider, &profile, disabled.Client())
	if err != nil || !profile.S3.Empty() || !profile.BrokerS3Disabled {
		t.Fatalf("disabled Broker S3 profile = %#v, err = %v", profile, err)
	}

	broken := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) { http.Error(w, "broken", http.StatusBadGateway) }))
	defer broken.Close()
	provider.Broker.Endpoint = broken.URL
	err = populateBrokerStorageForLogin(context.Background(), provider, &profile, broken.Client())
	if err == nil {
		t.Fatal("broken discovery was accepted")
	}
}

func TestLocalProviderLoginProfilesAndRedaction(t *testing.T) {
	globalDir := t.TempDir()
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), globalDir)
	t.Setenv("GRAPHIT_MODULES_DAEMON", "false")

	if out, err := executeCommand("--non-interactive", "provider", "add", "local", "--type", "local", "--mcp-endpoint", "http://mcp.example"); err != nil {
		t.Fatalf("provider add: %v\n%s", err, out)
	}
	if out, err := executeCommand("--non-interactive", "login", "--profile", "alice", "--provider", "local", "--username", "alice", "--mcp-key", "mcp-secret"); err != nil {
		t.Fatalf("login: %v\n%s", err, out)
	}
	out, err := executeCommand("--non-interactive", "account", "show", "alice")
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(out, "mcp-secret") || !strings.Contains(out, "[redacted]") {
		t.Fatalf("profile output was not redacted:\n%s", out)
	}

	if out, err := executeCommand("--non-interactive", "provider", "add", "other", "--type", "local"); err != nil {
		t.Fatalf("second provider: %v\n%s", err, out)
	}
	if out, err := executeCommand("--non-interactive", "login", "--profile", "bob", "--provider", "other", "--username", "bob"); err != nil {
		t.Fatalf("second login: %v\n%s", err, out)
	}
	out, err = executeCommand("--non-interactive", "account", "list")
	if err != nil || !strings.Contains(out, "* bob") || !strings.Contains(out, "alice") {
		t.Fatalf("account list: %v\n%s", err, out)
	}
	if out, err := executeCommand("--non-interactive", "account", "use", "alice"); err != nil {
		t.Fatalf("account use: %v\n%s", err, out)
	}
}

func TestNonInteractiveMissingInputAndDestructiveConsentFail(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	t.Setenv("GRAPHIT_MODULES_DAEMON", "false")
	if _, err := executeCommand("--non-interactive", "provider", "add", "missing"); err == nil || !strings.Contains(err.Error(), "required in non-interactive") {
		t.Fatalf("expected missing-input error, got %v", err)
	}
	if _, err := executeCommand("--non-interactive", "provider", "add", "temporary", "--type", "local"); err != nil {
		t.Fatal(err)
	}
	if _, err := executeCommand("--non-interactive", "provider", "remove", "temporary"); err == nil || !strings.Contains(err.Error(), "--yes") {
		t.Fatalf("expected --yes error, got %v", err)
	}
	if _, err := executeCommand("--non-interactive", "provider", "remove", "temporary", "--yes"); err != nil {
		t.Fatal(err)
	}
}

func TestProviderAIFlagsAndLoginSecrets(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	t.Setenv("GRAPHIT_MODULES_DAEMON", "false")
	out, err := executeCommand("--non-interactive", "provider", "add", "broker", "--type", "local",
		"--broker-endpoint", "http://127.0.0.1:8080", "--embedding-mode", "broker", "--rerank-mode", "broker")
	if err != nil {
		t.Fatalf("provider add: %v\n%s", err, out)
	}
	out, err = executeCommand("--non-interactive", "login", "--profile", "broker-user", "--provider", "broker", "--username", "alice", "--broker-key", "broker-secret")
	if err != nil {
		t.Fatalf("login: %v\n%s", err, out)
	}
	out, err = executeCommand("--non-interactive", "account", "show", "broker-user")
	if err != nil || strings.Contains(out, "broker-secret") || !strings.Contains(out, "[redacted]") {
		t.Fatalf("account show: %v\n%s", err, out)
	}

	out, err = executeCommand("--non-interactive", "provider", "add", "direct", "--type", "local",
		"--embedding-mode", "direct", "--embedding-protocol", "openai-embeddings-v1", "--embedding-endpoint", "https://api.example/v1", "--embedding-model", "embed-v1", "--embedding-dimensions", "1024",
		"--rerank-mode", "direct", "--rerank-protocol", "cohere", "--rerank-endpoint", "https://rerank.example", "--rerank-model", "rerank-v1")
	if err != nil {
		t.Fatalf("direct provider add: %v\n%s", err, out)
	}
	if _, err = executeCommand("--non-interactive", "login", "--profile", "direct-user", "--provider", "direct", "--username", "alice"); err == nil {
		t.Fatal("direct login without API keys succeeded")
	}
	out, err = executeCommand("--non-interactive", "login", "--profile", "direct-user", "--provider", "direct", "--username", "alice", "--embedding-api-key", "embed-secret", "--rerank-api-key", "rerank-secret")
	if err != nil {
		t.Fatalf("direct login: %v\n%s", err, out)
	}
}

func TestBrokerAuthenticationProviderDiscoversLoginAndRejectsStaticIdentityFlags(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	t.Setenv("GRAPHIT_MODULES_DAEMON", "false")
	out, err := executeCommand("--non-interactive", "provider", "add", "company", "--type", "broker", "--broker-endpoint", "http://127.0.0.1:8080")
	if err != nil {
		t.Fatalf("broker provider add: %v\n%s", err, out)
	}
	store, err := auth.Open()
	if err != nil {
		t.Fatal(err)
	}
	state, err := store.Load()
	if err != nil {
		t.Fatal(err)
	}
	provider := state.Providers["company"]
	if provider.Type != auth.ProviderBroker || provider.Broker == nil || provider.Broker.Endpoint != "http://127.0.0.1:8080" || provider.AI.Embedding.Mode != auth.ServiceBroker || provider.AI.Rerank.Mode != auth.ServiceBroker {
		t.Fatalf("broker provider=%#v", provider)
	}
	if _, err := executeCommand("--non-interactive", "login", "--profile", "alice", "--provider", "company"); err == nil || !strings.Contains(err.Error(), "interactive browser") {
		t.Fatalf("non-interactive broker login err=%v", err)
	}
	if _, err := executeCommand("--non-interactive", "provider", "add", "invalid", "--type", "broker", "--broker-endpoint", "http://127.0.0.1:8080", "--issuer", "https://identity.example"); err == nil || !strings.Contains(err.Error(), "do not accept upstream OIDC") {
		t.Fatalf("broker provider accepted OIDC flags: %v", err)
	}
}

func TestProviderPersistsEmbeddingSimulatedRerankDimensions(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	t.Setenv("GRAPHIT_MODULES_DAEMON", "false")
	out, err := executeCommand("--non-interactive", "provider", "add", "gemini", "--type", "local",
		"--embedding-mode", "disabled",
		"--rerank-mode", "direct", "--rerank-protocol", "google", "--rerank-endpoint", "https://generativelanguage.googleapis.com/v1beta", "--rerank-model", "gemini-embedding-001", "--rerank-dimensions", "3072")
	if err != nil {
		t.Fatalf("provider add: %v\n%s", err, out)
	}
	store, err := auth.Open()
	if err != nil {
		t.Fatal(err)
	}
	state, err := store.Load()
	if err != nil {
		t.Fatal(err)
	}
	service := state.Providers["gemini"].AI.Rerank
	if service.Protocol != "google" || service.Dimensions != 3072 {
		t.Fatalf("rerank service = %#v", service)
	}
	if _, err := executeCommand("--non-interactive", "provider", "add", "disabled-rerank", "--type", "local", "--rerank-mode", "disabled", "--rerank-dimensions", "768"); err == nil || !strings.Contains(err.Error(), "direct rerank flags") {
		t.Fatalf("non-direct rerank dimensions error = %v", err)
	}
}

func TestBrokerProviderRequiresBrokerAIFlags(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	t.Setenv("GRAPHIT_MODULES_DAEMON", "false")
	out, err := executeCommand("--non-interactive", "provider", "add", "invalid-broker", "--type", "local",
		"--broker-endpoint", "http://127.0.0.1:8080")
	if err == nil || !strings.Contains(err.Error(), "requires embedding and rerank modes to be broker") {
		t.Fatalf("broker provider without broker AI modes: err=%v\n%s", err, out)
	}
}

func TestNonInteractiveAnonymousLoginActivatesAnonymousBrokerAIWithoutSecrets(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	t.Setenv("GRAPHIT_MODULES_DAEMON", "false")
	out, err := executeCommand("--non-interactive", "provider", "add", "public-hub", "--type", "local",
		"--broker-endpoint", "http://127.0.0.1:8080", "--broker-allow-anonymous", "--embedding-mode", "broker", "--rerank-mode", "broker")
	if err != nil {
		t.Fatalf("provider add: %v\n%s", err, out)
	}
	out, err = executeCommand("--non-interactive", "login", "--profile", "public", "--provider", "public-hub", "--anonymous")
	if err != nil {
		t.Fatalf("anonymous login: %v\n%s", err, out)
	}
	out, err = executeCommand("--non-interactive", "account", "show", "public")
	if err != nil || !strings.Contains(out, `"username": "anonymous"`) || strings.Contains(out, "access_key_id") || strings.Contains(out, "broker_key") {
		t.Fatalf("anonymous account: %v\n%s", err, out)
	}
}

func TestLocalS3TopologyAndCredentialFlagsPersistRedacted(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	t.Setenv("GRAPHIT_MODULES_DAEMON", "false")
	if out, err := executeCommand("--non-interactive", "provider", "add", "direct-s3", "--type", "local",
		"--s3-bucket", "artifacts", "--s3-region", "us-east-1", "--s3-endpoint", "http://127.0.0.1:9000",
		"--s3-prefix", "graphit", "--s3-credential-source", "login"); err != nil {
		t.Fatalf("provider add: %v\n%s", err, out)
	}
	if out, err := executeCommand("--non-interactive", "login", "--profile", "direct", "--provider", "direct-s3",
		"--username", "alice", "--s3-access-key", "local-access", "--s3-secret-key", "local-secret", "--s3-session-token", "local-token"); err != nil {
		t.Fatalf("login: %v\n%s", err, out)
	}
	out, err := executeCommand("--non-interactive", "account", "show", "direct")
	if err != nil || !strings.Contains(out, `"access_key_id": "[redacted]"`) || strings.Contains(out, "local-access") || strings.Contains(out, "local-secret") || strings.Contains(out, "local-token") {
		t.Fatalf("redacted account: %v\n%s", err, out)
	}
	store, err := auth.Open()
	if err != nil {
		t.Fatal(err)
	}
	snapshot, err := store.Active()
	if err != nil || snapshot.Provider.S3.Bucket != "artifacts" || snapshot.Provider.S3.Prefix != "graphit" || snapshot.Profile.S3.AccessKeyID != "local-access" || snapshot.Profile.S3.SessionToken != "local-token" {
		t.Fatalf("persisted S3 state=%#v err=%v", snapshot, err)
	}
}

func TestProviderLocalONNXDefaultsAndIndependentSettings(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	t.Setenv("GRAPHIT_MODULES_DAEMON", "false")
	if out, err := executeCommand("--non-interactive", "provider", "add", "defaults", "--type", "local"); err != nil {
		t.Fatalf("default provider add: %v\n%s", err, out)
	}
	if out, err := executeCommand("--non-interactive", "provider", "add", "tuned", "--type", "local",
		"--embedding-device", "cuda", "--embedding-device-id", "1",
		"--rerank-device", "cpu", "--rerank-device-id", "0"); err != nil {
		t.Fatalf("tuned provider add: %v\n%s", err, out)
	}

	store, err := auth.Open()
	if err != nil {
		t.Fatal(err)
	}
	state, err := store.Load()
	if err != nil {
		t.Fatal(err)
	}
	defaults := state.Providers["defaults"]
	for name, service := range map[string]auth.AIServiceConfig{"embedding": defaults.AI.Embedding, "rerank": defaults.AI.Rerank} {
		if service.ONNX == nil || *service.ONNX != auth.DefaultONNXExecutionConfig() {
			t.Fatalf("%s default ONNX = %#v", name, service.ONNX)
		}
	}
	tuned := state.Providers["tuned"]
	if got := tuned.AI.Embedding.ONNX; got == nil || got.Device != auth.ONNXDeviceCUDA || got.DeviceID != 1 {
		t.Fatalf("embedding ONNX = %#v", got)
	}
	if got := tuned.AI.Rerank.ONNX; got == nil || got.Device != auth.ONNXDeviceCPU || got.DeviceID != 0 {
		t.Fatalf("rerank ONNX = %#v", got)
	}

	if out, err := executeCommand("--non-interactive", "provider", "update", "tuned", "--embedding-device", "cpu", "--embedding-device-id", "7"); err != nil {
		t.Fatalf("provider update: %v\n%s", err, out)
	}
	state, err = store.Load()
	if err != nil {
		t.Fatal(err)
	}
	tuned = state.Providers["tuned"]
	if got := tuned.AI.Embedding.ONNX; got == nil || got.Device != auth.ONNXDeviceCPU || got.DeviceID != 7 {
		t.Fatalf("updated embedding ONNX = %#v", got)
	}
	if got := tuned.AI.Rerank.ONNX; got == nil || got.Device != auth.ONNXDeviceCPU || got.DeviceID != 0 {
		t.Fatalf("rerank changed unexpectedly = %#v", got)
	}
}

func TestProviderLocalONNXAppliesToOIDCAuthentication(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	t.Setenv("GRAPHIT_MODULES_DAEMON", "false")
	out, err := executeCommand("--non-interactive", "provider", "add", "oidc-local", "--type", "oidc",
		"--issuer", "https://issuer.example", "--client-id", "client", "--username-claim", "preferred_username",
		"--embedding-device", "cpu", "--embedding-device-id", "2",
		"--rerank-device", "auto", "--rerank-device-id", "0")
	if err != nil {
		t.Fatalf("OIDC provider add: %v\n%s", err, out)
	}
	store, err := auth.Open()
	if err != nil {
		t.Fatal(err)
	}
	state, err := store.Load()
	if err != nil {
		t.Fatal(err)
	}
	provider := state.Providers["oidc-local"]
	if provider.Type != auth.ProviderOIDC || provider.AI.Embedding.ONNX == nil || provider.AI.Embedding.ONNX.Device != auth.ONNXDeviceCPU {
		t.Fatalf("OIDC local embedding configuration = %#v", provider)
	}
}

func TestProviderRejectsLocalONNXFlagsForNonLocalModes(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	t.Setenv("GRAPHIT_MODULES_DAEMON", "false")
	tests := [][]string{
		{"provider", "add", "disabled", "--type", "local", "--embedding-mode", "disabled", "--embedding-device", "cpu"},
		{"provider", "add", "direct", "--type", "local", "--embedding-mode", "direct", "--embedding-protocol", "openai-compatible", "--embedding-endpoint", "https://ai.example/v1", "--embedding-model", "embed", "--embedding-dimensions", "768", "--embedding-device", "cpu"},
		{"provider", "add", "broker", "--type", "local", "--broker-endpoint", "https://broker.example", "--embedding-mode", "broker", "--rerank-mode", "broker", "--rerank-device-id", "0"},
	}
	for _, args := range tests {
		args = append([]string{"--non-interactive"}, args...)
		if _, err := executeCommand(args...); err == nil || !strings.Contains(err.Error(), "require --") || !strings.Contains(err.Error(), "-mode=local") {
			t.Fatalf("args %v: error = %v", args, err)
		}
	}
}

func TestProviderLocalONNXInteractivePromptsPreserveCurrentValues(t *testing.T) {
	previous := nonInteractive
	nonInteractive = false
	t.Cleanup(func() { nonInteractive = previous })
	cmd := newProviderUpdateCmd()
	current := auth.ONNXExecutionConfig{Device: auth.ONNXDeviceCUDA, DeviceID: 3}
	service := auth.AIServiceConfig{Mode: auth.ServiceLocal, ONNX: &current}
	if err := configureProviderONNXExecution(cmd, bufio.NewReader(strings.NewReader("\n\n")), "embedding", &service, "", ""); err != nil {
		t.Fatal(err)
	}
	if service.ONNX == nil || *service.ONNX != current {
		t.Fatalf("ONNX after accepting current prompts = %#v", service.ONNX)
	}
}
