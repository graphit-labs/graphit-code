package commands

import (
	"testing"

	"github.com/graphit-labs/graphit-code/internal/auth"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
)

func TestSetupPersistsInstallationIdentityBeforeSuccess(t *testing.T) {
	globalDir := t.TempDir()
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), globalDir)
	t.Setenv(config.ConfigEnvVar(config.UnitIDKey), "")
	t.Setenv(config.ConfigEnvVar(config.ClientSecretConfigKey), "")
	t.Setenv("GRAPHIT_MODULES_DAEMON", "false")

	output, err := executeCommand(
		"--non-interactive",
		"setup",
		"--anonymize-events=false",
		"--agent=codex",
		"--cli=codex",
	)
	if err != nil {
		t.Fatalf("setup failed: %v\n%s", err, output)
	}
	for _, key := range []string{config.UnitIDKey, config.ClientSecretConfigKey} {
		value, ok, getErr := config.GetGlobalConfigValue(key)
		if getErr != nil || !ok || value == "" {
			t.Fatalf("setup persisted %s = %q, ok=%t, err=%v\n%s", key, value, ok, getErr, output)
		}
	}
	store, err := auth.Open()
	if err != nil {
		t.Fatal(err)
	}
	state, err := store.Load()
	if err != nil {
		t.Fatal(err)
	}
	provider, ok := state.Providers[auth.DefaultLocalProviderName]
	if !ok || provider.Type != auth.ProviderLocal || provider.Local == nil {
		t.Fatalf("setup did not persist the default local provider: %#v\n%s", state, output)
	}
	if state.ActiveProfile != "" || len(state.Profiles) != 0 {
		t.Fatalf("setup created an artificial profile: %#v", state)
	}
	for name, service := range map[string]auth.AIServiceConfig{"embedding": provider.AI.Embedding, "rerank": provider.AI.Rerank} {
		if service.Mode != auth.ServiceLocal || service.ONNX == nil || *service.ONNX != auth.DefaultONNXExecutionConfig() {
			t.Fatalf("setup %s service = %#v", name, service)
		}
	}
}

func TestRepeatedSetupPreservesActiveProfile(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	t.Setenv(config.ConfigEnvVar(config.UnitIDKey), "")
	t.Setenv(config.ConfigEnvVar(config.ClientSecretConfigKey), "")
	t.Setenv("GRAPHIT_MODULES_DAEMON", "false")
	store, err := auth.Open()
	if err != nil {
		t.Fatal(err)
	}
	chosen := auth.Provider{
		Name: "chosen", Type: auth.ProviderLocal, Local: &auth.LocalConfig{},
		AI: auth.AIConfig{
			Embedding: auth.AIServiceConfig{Mode: auth.ServiceDisabled},
			Rerank:    auth.AIServiceConfig{Mode: auth.ServiceDisabled},
		},
	}
	if err := store.AddProvider(chosen); err != nil {
		t.Fatal(err)
	}
	if err := store.Login(auth.Profile{Name: "alice", Provider: "chosen", Username: "alice"}); err != nil {
		t.Fatal(err)
	}

	output, err := executeCommand("--non-interactive", "setup", "--anonymize-events=false", "--agent=codex", "--cli=codex")
	if err != nil {
		t.Fatalf("setup failed: %v\n%s", err, output)
	}
	state, err := store.Load()
	if err != nil {
		t.Fatal(err)
	}
	if state.ActiveProfile != "alice" || state.Profiles["alice"].Provider != "chosen" {
		t.Fatalf("setup changed active selection: %#v", state)
	}
	if state.Providers["chosen"].AI.Embedding.Mode != auth.ServiceDisabled {
		t.Fatalf("setup changed chosen provider: %#v", state.Providers["chosen"])
	}
	if _, ok := state.Providers[auth.DefaultLocalProviderName]; !ok {
		t.Fatal("setup did not ensure the default local provider")
	}
}

func TestSetupCompletesMissingLocalDefaultsWithoutOverwritingConfiguration(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	t.Setenv(config.ConfigEnvVar(config.UnitIDKey), "")
	t.Setenv(config.ConfigEnvVar(config.ClientSecretConfigKey), "")
	t.Setenv("GRAPHIT_MODULES_DAEMON", "false")
	store, err := auth.Open()
	if err != nil {
		t.Fatal(err)
	}
	cpu := auth.ONNXExecutionConfig{Device: auth.ONNXDeviceCPU, DeviceID: 4}
	if err := store.AddProvider(auth.Provider{
		Name: auth.DefaultLocalProviderName, Type: auth.ProviderLocal,
		Local: &auth.LocalConfig{AllowAWSCredentialChain: true},
		AI: auth.AIConfig{
			Embedding: auth.AIServiceConfig{Mode: auth.ServiceLocal, ONNX: &cpu},
		},
	}); err != nil {
		t.Fatal(err)
	}

	output, err := executeCommand("--non-interactive", "setup", "--anonymize-events=false", "--agent=codex", "--cli=codex")
	if err != nil {
		t.Fatalf("setup failed: %v\n%s", err, output)
	}
	state, err := store.Load()
	if err != nil {
		t.Fatal(err)
	}
	provider := state.Providers[auth.DefaultLocalProviderName]
	if provider.Local == nil || !provider.Local.AllowAWSCredentialChain || provider.AI.Embedding.ONNX == nil || *provider.AI.Embedding.ONNX != cpu {
		t.Fatalf("setup overwrote explicit local configuration: %#v", provider)
	}
	if provider.AI.Rerank.Mode != auth.ServiceLocal || provider.AI.Rerank.ONNX == nil || *provider.AI.Rerank.ONNX != auth.DefaultONNXExecutionConfig() {
		t.Fatalf("setup did not complete missing rerank defaults: %#v", provider.AI.Rerank)
	}
}
