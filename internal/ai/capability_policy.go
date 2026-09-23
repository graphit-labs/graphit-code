package ai

import (
	"fmt"
	"os"
	"runtime"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/agentpolicy"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/daemonctl"
)

var capabilityRuntimeKey = readCapabilityRuntimeKey

var dreamEnvironmentNames = map[string]bool{
	"PATH": true, "HOME": true, "USER": true, "LOGNAME": true, "SHELL": true,
	"USERPROFILE": true, "HOMEDRIVE": true, "HOMEPATH": true,
	"APPDATA": true, "LOCALAPPDATA": true, "PROGRAMDATA": true,
	"SYSTEMROOT": true, "WINDIR": true, "COMSPEC": true, "PATHEXT": true,
	"TEMP": true, "TMP": true, "TMPDIR": true,
	"LANG": true, "LANGUAGE": true, "TZ": true,
	"XDG_CONFIG_HOME": true, "XDG_DATA_HOME": true, "XDG_CACHE_HOME": true,
	"XDG_STATE_HOME": true, "XDG_RUNTIME_DIR": true,
	"HTTP_PROXY": true, "HTTPS_PROXY": true, "ALL_PROXY": true, "NO_PROXY": true,
	"http_proxy": true, "https_proxy": true, "all_proxy": true, "no_proxy": true,
	"SSL_CERT_FILE": true, "SSL_CERT_DIR": true, "REQUESTS_CA_BUNDLE": true,
	"CURL_CA_BUNDLE": true, "NODE_EXTRA_CA_CERTS": true,
	"CODEX_HOME": true, "CLAUDE_CONFIG_DIR": true, "GEMINI_CLI_HOME": true,
	"OPENCODE_CONFIG": true, "OPENCODE_CONFIG_DIR": true, "QWEN_CODE_HOME": true,
	"KIMI_CLI_HOME": true, "GITHUB_CONFIG_DIR": true,
	"ANTHROPIC_API_KEY": true, "ANTHROPIC_AUTH_TOKEN": true, "ANTHROPIC_BASE_URL": true,
	"OPENAI_API_KEY": true, "OPENAI_BASE_URL": true, "AZURE_OPENAI_API_KEY": true,
	"AZURE_OPENAI_ENDPOINT": true, "GEMINI_API_KEY": true, "GOOGLE_API_KEY": true,
	"GOOGLE_APPLICATION_CREDENTIALS": true, "GOOGLE_CLOUD_PROJECT": true,
	"GOOGLE_CLOUD_LOCATION": true, "GOOGLE_GENAI_USE_VERTEXAI": true,
	"XAI_API_KEY": true, "GROK_API_KEY": true, "DASHSCOPE_API_KEY": true,
	"QWEN_API_KEY": true, "KIMI_API_KEY": true, "MOONSHOT_API_KEY": true,
	"GH_TOKEN": true, "GITHUB_TOKEN": true,
	"AWS_ACCESS_KEY_ID": true, "AWS_SECRET_ACCESS_KEY": true, "AWS_SESSION_TOKEN": true,
	"AWS_PROFILE": true, "AWS_REGION": true, "AWS_DEFAULT_REGION": true,
}

func processEnvironment(req StreamRequest) []string {
	environ := os.Environ()
	if req.Capabilities == nil {
		return environ
	}
	allowed := make([]string, 0, len(environ))
	globalDirEnv := brand.EnvVar("GLOBAL_DIR")
	launcherEnv := brand.EnvVar("LAUNCHER_PATH")
	for _, item := range environ {
		name, _, ok := strings.Cut(item, "=")
		if !ok {
			continue
		}
		if dreamEnvironmentAllowed(name, globalDirEnv, launcherEnv, runtime.GOOS == "windows") {
			allowed = append(allowed, item)
		}
	}
	return allowed
}

func dreamEnvironmentAllowed(name, globalDirEnv, launcherEnv string, windows bool) bool {
	if dreamEnvironmentNames[name] || strings.HasPrefix(name, "LC_") || name == globalDirEnv || name == launcherEnv {
		return true
	}
	if !windows {
		return false
	}
	if strings.EqualFold(name, globalDirEnv) || strings.EqualFold(name, launcherEnv) || strings.HasPrefix(strings.ToUpper(name), "LC_") {
		return true
	}
	for allowed := range dreamEnvironmentNames {
		if strings.EqualFold(name, allowed) {
			return true
		}
	}
	return false
}

func validateCapabilityRequestEnvironment(req StreamRequest) error {
	if req.Capabilities == nil {
		return nil
	}
	for name := range req.Env {
		if name != agentpolicy.EnvRunID && name != "GRAPHIT_UNIT_ID" {
			return fmt.Errorf("dream capability policy rejects request environment variable %q", name)
		}
	}
	return nil
}

func readCapabilityRuntimeKey() (string, string, error) {
	if _, err := daemonctl.EnsureRunning(); err != nil {
		return "", "", fmt.Errorf("starting Graphit daemon: %w", err)
	}
	keyPath := daemonctl.KeyFilePath()
	deadline := time.Now().Add(5 * time.Second)
	for {
		data, err := os.ReadFile(keyPath)
		if err == nil && strings.TrimSpace(string(data)) != "" {
			return strings.TrimSpace(string(data)), keyPath, nil
		}
		if time.Now().After(deadline) {
			if err == nil {
				err = fmt.Errorf("runtime key is empty")
			}
			return "", "", fmt.Errorf("reading Graphit daemon runtime key: %w", err)
		}
		time.Sleep(20 * time.Millisecond)
	}
}

func useConfiguredAgentArgs(req StreamRequest) bool {
	return req.AllowTools && req.Capabilities == nil
}

// RestrictNativeToolsWhenAvailable requests native tool controls. CLIs without
// a verified control still run, but may edit files; this is not an OS jail.
func DreamMemoryCapabilities() *CapabilityPolicy {
	return &CapabilityPolicy{MCPProfile: agentpolicy.ProfileDreamMemory, RestrictNativeToolsWhenAvailable: true}
}

// A Dream policy is enforced by the CLI's own tool controls. The CLI process
// retains its ordinary state/cache writes; only model-invoked tools are limited.
// Graphit MCP separately authenticates and restricts its own tool catalog.
func (c *cliClient) applyCapabilityPolicy(req StreamRequest, args []string) (string, []string, map[string]string, func(), error) {
	policy := req.Capabilities
	if policy == nil {
		return c.executablePath, args, nil, nil, nil
	}
	if _, supported := knownCLIs[c.binaryName]; !supported {
		return "", nil, nil, nil, fmt.Errorf("CLI %q is not a supported Dream agent", c.binaryName)
	}
	if policy.MCPProfile != agentpolicy.ProfileDreamMemory || !policy.RestrictNativeToolsWhenAvailable {
		return "", nil, nil, nil, fmt.Errorf("CLI %q received an unsupported capability policy", c.binaryName)
	}
	if err := validateCapabilityRequestEnvironment(req); err != nil {
		return "", nil, nil, nil, err
	}
	// Mint before launch. Never put the daemon master key in the child environment.
	runtimeKey, _, err := capabilityRuntimeKey()
	if err != nil {
		return "", nil, nil, nil, fmt.Errorf("CLI %q cannot create a scoped Dream credential: %w", c.binaryName, err)
	}
	token, err := agentpolicy.MintCapabilityToken(runtimeKey, policy.MCPProfile)
	if err != nil {
		return "", nil, nil, nil, fmt.Errorf("CLI %q cannot create a scoped Dream credential: %w", c.binaryName, err)
	}
	policyArgs, policyEnv, cleanup, err := nativeDreamPolicy(c.binaryName, args)
	if err != nil {
		return "", nil, nil, nil, err
	}
	policyEnv[agentpolicy.EnvProfile] = policy.MCPProfile
	policyEnv[agentpolicy.EnvCapabilityToken] = token
	return c.executablePath, policyArgs, policyEnv, cleanup, nil
}

const geminiDreamPolicyFormat = `[[rule]]
toolName = "*"
decision = "deny"
priority = 900
denyMessage = "Dream only allows read tools and Graphit Memory MCP tools"

[[rule]]
toolName = ["read_file", "read_many_files", "glob", "grep_search", "list_directory"]
decision = "allow"
priority = 950

[[rule]]
mcpName = %q
toolName = "*"
decision = "allow"
priority = 950
`

const openCodeDreamConfigFormat = `{"agent":{"graphit-dream":{"description":"Graphit Dream memory consolidation","mode":"primary","permission":{"*":"deny","read":"allow","glob":"allow","grep":"allow","list":"allow",%q:"allow"}}}}`

// Kimi's custom agent file replaces the built-in tool list with read-only
// tools. MCP servers are loaded separately by the CLI.
const kimiDreamAgent = `version: 1
agent:
  extend: default
  name: graphit-dream
  tools:
    - "kimi_cli.tools.file:ReadFile"
    - "kimi_cli.tools.file:ReadMediaFile"
    - "kimi_cli.tools.file:Glob"
    - "kimi_cli.tools.file:Grep"
`

// nativeDreamPolicy returns per-invocation CLI controls where verified. Other
// known CLIs run without native restrictions, so their own tools may edit files.
// A temporary policy is removed after process exit or command-start failure.
func nativeDreamPolicy(binary string, args []string) ([]string, map[string]string, func(), error) {
	env := make(map[string]string)
	switch binary {
	case "claude":
		return append([]string{"--restricted", "--tools", "Read,Glob,Grep"}, args...), env, nil, nil
	case "gemini":
		serverName := brand.MCPServerName("code-stdio")
		path, cleanup, err := writeDreamPolicy(".toml", fmt.Sprintf(geminiDreamPolicyFormat, serverName))
		if err != nil {
			return nil, nil, nil, fmt.Errorf("creating Gemini Dream policy: %w", err)
		}
		return append([]string{"--approval-mode", "default", "--policy", path, "--allowed-mcp-server-names", serverName}, args...), env, cleanup, nil
	case "codex":
		return insertBeforePrompt(args, "--sandbox", "read-only", "--ask-for-approval", "never"), env, nil, nil
	case "opencode":
		toolPrefix := strings.ReplaceAll(brand.MCPServerName("code-stdio"), "-", "_") + "_*"
		env["OPENCODE_CONFIG_CONTENT"] = fmt.Sprintf(openCodeDreamConfigFormat, toolPrefix)
		return insertBeforePrompt(args, "--agent", "graphit-dream"), env, nil, nil
	case "kimi":
		path, cleanup, err := writeDreamPolicy(".yaml", kimiDreamAgent)
		if err != nil {
			return nil, nil, nil, fmt.Errorf("creating Kimi Dream agent policy: %w", err)
		}
		return append([]string{"--agent-file", path}, args...), env, cleanup, nil
	default:
		// The caller already checked knownCLIs. The user explicitly accepts the
		// file-write risk when this CLI lacks a verified per-run control.
		return args, env, nil, nil
	}
}

func insertBeforePrompt(args []string, options ...string) []string {
	if len(args) == 0 {
		return append([]string(nil), options...)
	}
	last := len(args) - 1
	out := make([]string, 0, len(args)+len(options))
	out = append(out, args[:last]...)
	out = append(out, options...)
	return append(out, args[last])
}

func writeDreamPolicy(suffix, contents string) (string, func(), error) {
	f, err := os.CreateTemp("", brand.TempDirPrefix("dream-policy")+"*"+suffix)
	if err != nil {
		return "", nil, err
	}
	cleanup := func() { _ = os.Remove(f.Name()) }
	if _, err := f.WriteString(contents); err != nil {
		_ = f.Close()
		cleanup()
		return "", nil, err
	}
	if err := f.Close(); err != nil {
		cleanup()
		return "", nil, err
	}
	return f.Name(), cleanup, nil
}
