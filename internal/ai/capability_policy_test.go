package ai

import (
	"context"
	"errors"
	"os"
	"os/exec"
	"strings"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/agentpolicy"
	"github.com/graphit-labs/graphit-code/internal/brand"
)

func stubDreamRuntimeKey(t *testing.T) {
	t.Helper()
	old := capabilityRuntimeKey
	capabilityRuntimeKey = func() (string, string, error) { return "runtime-key", "/daemon/mcp.key", nil }
	t.Cleanup(func() { capabilityRuntimeKey = old })
}

func TestDreamNativeCapabilityPolicies(t *testing.T) {
	stubDreamRuntimeKey(t)
	tests := []struct {
		name, wantArg, wantEnv string
	}{
		{"claude", "--restricted", ""},
		{"gemini", "--policy", ""},
		{"codex", "read-only", ""},
		{"opencode", "", "OPENCODE_CONFIG_CONTENT"},
		{"kimi", "--agent-file", ""},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			client := &cliClient{binaryName: tt.name, executablePath: "/agents/" + tt.name}
			inputArgs := []string{"--structured", "-p", "-"}
			switch tt.name {
			case "codex":
				inputArgs = []string{"exec", "--json", "-"}
			case "opencode":
				inputArgs = []string{"run", "--format", "json", "prompt"}
			}
			executable, args, env, cleanup, err := client.applyCapabilityPolicy(StreamRequest{
				Capabilities: DreamMemoryCapabilities(),
			}, inputArgs)
			if err != nil {
				t.Fatal(err)
			}
			if cleanup != nil {
				defer cleanup()
			}
			if executable != client.executablePath {
				t.Fatalf("executable = %q, want native CLI %q", executable, client.executablePath)
			}
			if args[len(args)-1] != inputArgs[len(inputArgs)-1] {
				t.Fatalf("policy options placed after prompt: %v", args)
			}
			if tt.name == "claude" || tt.name == "gemini" || tt.name == "kimi" {
				if args[len(args)-2] != "-p" {
					t.Fatalf("policy options split -p from its stdin argument: %v", args)
				}
			}
			if tt.wantArg != "" && !strings.Contains(strings.Join(args, " "), tt.wantArg) {
				t.Fatalf("missing native policy %q: %v", tt.wantArg, args)
			}
			if tt.wantEnv != "" && env[tt.wantEnv] == "" {
				t.Fatalf("missing %s policy environment", tt.wantEnv)
			}
			if profile, ok := agentpolicy.VerifyCapabilityToken("runtime-key", env[agentpolicy.EnvCapabilityToken]); !ok || profile != agentpolicy.ProfileDreamMemory {
				t.Fatal("Dream bearer is not scoped")
			}
			if env[agentpolicy.EnvProfile] != agentpolicy.ProfileDreamMemory {
				t.Fatalf("wrong MCP profile: %v", env)
			}
			if tt.name == "gemini" {
				idx := -1
				for i, arg := range args {
					if arg == "--policy" {
						idx = i
						break
					}
				}
				if idx < 0 || idx+1 >= len(args) {
					t.Fatalf("missing Gemini policy path: %v", args)
				}
				data, err := os.ReadFile(args[idx+1])
				if err != nil {
					t.Fatal(err)
				}
				if !strings.Contains(string(data), `mcpName = "`+brand.MCPServerName("code-stdio")+`"`) || !strings.Contains(string(data), `toolName = "*"`) {
					t.Fatalf("Gemini policy does not default-deny and allow Graphit MCP: %s", data)
				}
				cleanup()
				if _, err := os.Stat(args[idx+1]); !os.IsNotExist(err) {
					t.Fatalf("Gemini policy was not removed: %v", err)
				}
			}
			if tt.name == "opencode" {
				toolPrefix := strings.ReplaceAll(brand.MCPServerName("code-stdio"), "-", "_") + "_*"
				if !strings.Contains(env["OPENCODE_CONFIG_CONTENT"], `"*":"deny"`) || !strings.Contains(env["OPENCODE_CONFIG_CONTENT"], `"`+toolPrefix+`":"allow"`) || !strings.Contains(strings.Join(args, " "), "--agent graphit-dream") {
					t.Fatalf("OpenCode Dream agent policy missing: args=%v env=%v", args, env)
				}
			}
			if tt.name == "kimi" {
				if len(args) < 2 || args[0] != "--agent-file" {
					t.Fatalf("Kimi agent file missing: %v", args)
				}
				data, err := os.ReadFile(args[1])
				if err != nil {
					t.Fatal(err)
				}
				if !strings.Contains(string(data), "ReadFile") || strings.Contains(string(data), "WriteFile") || strings.Contains(string(data), "Shell") {
					t.Fatalf("Kimi policy does not restrict built-ins: %s", data)
				}
			}
		})
	}
}

func TestDreamCLIsWithoutNativePolicyRemainUsableWithScopedMCP(t *testing.T) {
	stubDreamRuntimeKey(t)
	for _, name := range []string{"agy", "qwen", "grok", "cursor-agent", "kiro-cli", "copilot"} {
		t.Run(name, func(t *testing.T) {
			client := &cliClient{binaryName: name, executablePath: "/agents/" + name}
			inputArgs := []string{"--structured", "prompt"}
			executable, args, env, cleanup, err := client.applyCapabilityPolicy(StreamRequest{Capabilities: DreamMemoryCapabilities()}, inputArgs)
			if err != nil || executable != client.executablePath || cleanup != nil {
				t.Fatalf("CLI %s failed fallback: executable=%q cleanup=%v err=%v", name, executable, cleanup != nil, err)
			}
			if strings.Join(args, "\x00") != strings.Join(inputArgs, "\x00") {
				t.Fatalf("CLI %s got unsupported native flags: %v", name, args)
			}
			if profile, ok := agentpolicy.VerifyCapabilityToken("runtime-key", env[agentpolicy.EnvCapabilityToken]); !ok || profile != agentpolicy.ProfileDreamMemory {
				t.Fatalf("CLI %s lost scoped MCP bearer", name)
			}
		})
	}
}

func TestDreamUnknownCLIFailsClosed(t *testing.T) {
	stubDreamRuntimeKey(t)
	client := &cliClient{binaryName: "custom-agent", executablePath: "/agents/custom-agent"}
	if _, _, _, _, err := client.applyCapabilityPolicy(StreamRequest{Capabilities: DreamMemoryCapabilities()}, nil); err == nil || !strings.Contains(err.Error(), "not a supported Dream agent") {
		t.Fatalf("unknown CLI = %v", err)
	}
}

func TestDreamProcessEnvironmentDoesNotInheritGenericBearer(t *testing.T) {
	t.Setenv("OIDC_ACCESS_TOKEN", "unrestricted-graphit-token")
	t.Setenv("RANDOM_SECRET", "also-not-inherited")
	t.Setenv("OPENAI_API_KEY", "model-provider-token")
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	env := strings.Join(processEnvironment(StreamRequest{Capabilities: DreamMemoryCapabilities()}), "\n")
	if strings.Contains(env, "unrestricted-graphit-token") || strings.Contains(env, "also-not-inherited") {
		t.Fatalf("Dream inherited an unapproved environment secret: %s", env)
	}
	for _, want := range []string{"OPENAI_API_KEY=model-provider-token", brand.EnvVar("GLOBAL_DIR") + "="} {
		if !strings.Contains(env, want) {
			t.Fatalf("Dream environment omitted %q: %s", want, env)
		}
	}
}

func TestDreamProcessEnvironmentPreservesWindowsAndMacRuntimePaths(t *testing.T) {
	globalDir := brand.EnvVar("GLOBAL_DIR")
	launcher := brand.EnvVar("LAUNCHER_PATH")
	for _, name := range []string{"APPDATA", "LOCALAPPDATA", "USERPROFILE", "SYSTEMROOT", "TEMP", "TMPDIR"} {
		if !dreamEnvironmentAllowed(name, globalDir, launcher, false) {
			t.Fatalf("Dream environment lost runtime path %q", name)
		}
	}
	for _, name := range []string{"Path", "AppData", strings.ToLower(globalDir)} {
		if !dreamEnvironmentAllowed(name, globalDir, launcher, true) {
			t.Fatalf("Dream environment lost Windows case-insensitive variable %q", name)
		}
	}
	if dreamEnvironmentAllowed("Path", globalDir, launcher, false) || dreamEnvironmentAllowed("OIDC_ACCESS_TOKEN", globalDir, launcher, true) {
		t.Fatal("Dream environment widened outside platform-specific runtime names")
	}
}

func TestDreamCapabilityPolicyRejectsCallerEnvironmentExpansion(t *testing.T) {
	client := &cliClient{binaryName: "codex", executablePath: "/agents/codex"}
	_, _, _, _, err := client.applyCapabilityPolicy(StreamRequest{
		Capabilities: DreamMemoryCapabilities(), Env: map[string]string{"OIDC_ACCESS_TOKEN": "bypass"},
	}, nil)
	if err == nil || !strings.Contains(err.Error(), "rejects request environment") {
		t.Fatalf("unexpected environment validation error: %v", err)
	}
}

func TestDreamCapabilityPolicyFailsClosedWithoutRuntimeKey(t *testing.T) {
	old := capabilityRuntimeKey
	capabilityRuntimeKey = func() (string, string, error) { return "", "", errors.New("missing key") }
	t.Cleanup(func() { capabilityRuntimeKey = old })
	client := &cliClient{binaryName: "codex", executablePath: "/agents/codex"}
	if _, _, _, _, err := client.applyCapabilityPolicy(StreamRequest{Capabilities: DreamMemoryCapabilities()}, nil); err == nil || !strings.Contains(err.Error(), "scoped Dream credential") {
		t.Fatalf("runtime key failure = %v", err)
	}
}

func TestDreamCapabilityFactoryRequestsNativeRestrictionsWhenAvailable(t *testing.T) {
	policy := DreamMemoryCapabilities()
	if !policy.RestrictNativeToolsWhenAvailable || policy.MCPProfile != agentpolicy.ProfileDreamMemory {
		t.Fatalf("unexpected Dream policy: %+v", policy)
	}
}

func TestDreamPolicyNeverIncludesConfiguredAgentArgs(t *testing.T) {
	if useConfiguredAgentArgs(StreamRequest{AllowTools: true, Capabilities: DreamMemoryCapabilities()}) {
		t.Fatal("Dream policy allowed ai.agent_args to widen its authority")
	}
	if !useConfiguredAgentArgs(StreamRequest{AllowTools: true}) {
		t.Fatal("ordinary agentic requests unexpectedly lost configured agent args")
	}
}

// Opt-in smoke test of installed native parsers; it makes no model request.
// It is not a substitute for an authenticated MCP tool-execution test.
func TestInstalledDreamCLIPolicyParsing(t *testing.T) {
	if os.Getenv("GRAPHIT_TEST_NATIVE_CLIS") != "1" {
		t.Skip("set GRAPHIT_TEST_NATIVE_CLIS=1 to inspect installed CLI policies")
	}
	if binary, err := exec.LookPath("gemini"); err == nil {
		args, _, cleanup, err := nativeDreamPolicy("gemini", nil)
		if err != nil {
			t.Fatal(err)
		}
		defer cleanup()
		ctx, cancel := context.WithTimeout(context.Background(), 20*time.Second)
		defer cancel()
		cmd := exec.CommandContext(ctx, binary, append(args, "--list-extensions")...)
		cmd.Env = append(os.Environ(), "GEMINI_API_KEY=graphit-dream-policy-parse-only")
		if output, err := cmd.CombinedOutput(); err != nil {
			t.Fatalf("Gemini rejected Dream policy: %v: %s", err, output)
		}
	}
}
