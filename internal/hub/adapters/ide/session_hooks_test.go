package ide

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/paths"
	"github.com/graphit-labs/graphit-code/internal/sessionhook"
)

func TestEveryAdapterInstallsOneOrderedSessionMemoryHook(t *testing.T) {
	t.Setenv("GRAPHIT_LAUNCHER_PATH", "/opt/graphit/bin/graphit")
	t.Setenv("HOME", t.TempDir())

	tests := []struct {
		adapter string
		path    string
		seed    string
		format  string
	}{
		{"antigravity", filepath.Join(".agents", "hooks.json"), `{"user-hook":{"Stop":[{"command":"user-token"}]}}`, sessionhook.FormatFirstInvocation},
		{"cursor", filepath.Join(".cursor", "hooks.json"), `{"version":1,"hooks":{"sessionStart":[{"command":"user-token"}]}}`, sessionhook.FormatAdditionalContext},
		{"claude", filepath.Join(".claude", "settings.json"), `{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"user-token"}]}]}}`, sessionhook.FormatSessionStart},
		{"kiro", filepath.Join(".kiro", "hooks", "graphit-memory.json"), `{"version":"v1","hooks":[{"name":"user-hook","trigger":"Stop","action":{"type":"command","command":"user-token"}}]}`, ""},
		{"codex", filepath.Join(".codex", "hooks.json"), `{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"user-token"}]}]}}`, sessionhook.FormatSessionStart},
		{"opencode", filepath.Join(".opencode", "plugins", opencodeManagedHookFile), "", ""},
		{"gemini", filepath.Join(".gemini", "settings.json"), `{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"user-token"}]}]},"userSetting":true}`, sessionhook.FormatSessionStart},
	}

	for _, tc := range tests {
		t.Run(tc.adapter, func(t *testing.T) {
			projectDir := t.TempDir()
			target := filepath.Join(projectDir, tc.path)
			if !strings.HasPrefix(target, projectDir+string(os.PathSeparator)) {
				t.Fatalf("hook target escaped project: %s", target)
			}
			if tc.seed != "" {
				if err := os.MkdirAll(filepath.Dir(target), 0o755); err != nil {
					t.Fatal(err)
				}
				if err := os.WriteFile(target, []byte(tc.seed), 0o644); err != nil {
					t.Fatal(err)
				}
			}
			if tc.adapter == "opencode" {
				userPlugin := filepath.Join(projectDir, ".opencode", "plugins", "user-plugin.js")
				if err := os.MkdirAll(filepath.Dir(userPlugin), 0o755); err != nil {
					t.Fatal(err)
				}
				if err := os.WriteFile(userPlugin, []byte("user-token"), 0o644); err != nil {
					t.Fatal(err)
				}
			}

			adapter := GetAdapter(tc.adapter)
			base, ok := folderBase(adapter)
			if !ok {
				t.Fatalf("adapter %q is not folder-backed", tc.adapter)
			}
			configuredTarget, err := resolveConfiguredPath(base.cfg.HookFilePath, projectDir)
			if err != nil {
				t.Fatal(err)
			}
			if configuredTarget != target {
				t.Fatalf("HookFilePath resolved to %q, want %q", configuredTarget, target)
			}
			if _, ok := adapter.(interface {
				syncSessionStartHook(string) error
				removeSessionStartHook(string) error
			}); !ok {
				t.Fatalf("adapter %q does not own its hook lifecycle", tc.adapter)
			}
			pp := &paths.ProjectPaths{ActiveProjectDir: projectDir}
			if err := adapter.Sync(map[string]map[string]string{}, pp, "hook-test-project"); err != nil {
				t.Fatal(err)
			}
			first, err := os.ReadFile(target)
			if err != nil {
				t.Fatal(err)
			}
			if err := adapter.Sync(map[string]map[string]string{}, pp, "hook-test-project"); err != nil {
				t.Fatal(err)
			}
			second, err := os.ReadFile(target)
			if err != nil {
				t.Fatal(err)
			}
			if string(first) != string(second) {
				t.Fatalf("second sync was not idempotent\nfirst: %s\nsecond: %s", first, second)
			}
			configContent := string(second)
			protocolContent := configContent
			if tc.adapter == "kiro" {
				payload, err := sessionhook.Render(sessionhook.FormatPlainContext, nil)
				if err != nil {
					t.Fatal(err)
				}
				protocolContent = string(payload)
			} else if tc.adapter != "opencode" {
				if strings.Count(configContent, "_session-hook --format "+tc.format) != 1 {
					t.Fatalf("expected one managed command hook: %s", configContent)
				}
				input := []byte(nil)
				if tc.adapter == "antigravity" {
					input = []byte(`{"invocationNum":0}`)
				}
				payload, err := sessionhook.Render(tc.format, input)
				if err != nil {
					t.Fatal(err)
				}
				protocolContent = string(payload)
			}
			if strings.Count(protocolContent, "graphit_memory_mandatory") != 1 {
				t.Fatalf("expected one mandatory recall instruction: %s", protocolContent)
			}
			if strings.Count(protocolContent, "exclude_mandatory: true") != 1 {
				t.Fatalf("expected one contextual exclusion instruction: %s", protocolContent)
			}
			if strings.Index(protocolContent, "graphit_memory_mandatory") >= strings.Index(protocolContent, "graphit_memory_search") {
				t.Fatalf("mandatory recall must precede contextual search: %s", protocolContent)
			}
			if !strings.Contains(protocolContent, "default native tools") {
				t.Fatalf("%s bootstrap must preserve native fallback when Graphit MCP tools are unavailable: %s", tc.adapter, protocolContent)
			}
			if tc.adapter == "opencode" && (!strings.Contains(configContent, `Bun.spawnSync`) || !strings.Contains(configContent, `experimental.chat.system.transform`) || !strings.Contains(configContent, `experimental.session.compacting`)) {
				t.Fatalf("OpenCode plugin must load memory and inject at model/compaction boundaries: %s", configContent)
			}
			if tc.adapter == "kiro" && strings.Count(configContent, "_session-hook --format plain-context") != 2 {
				t.Fatalf("Kiro must cover IDE SessionStart and CLI AgentSpawn: %s", configContent)
			}
			if tc.adapter == "gemini" && !strings.Contains(configContent, "BeforeAgent") {
				t.Fatalf("Gemini must reassert the invariant before every agent turn: %s", configContent)
			}
			if (tc.adapter == "claude" || tc.adapter == "codex") && !strings.Contains(configContent, "SubagentStart") {
				t.Fatalf("%s must bootstrap subagents: %s", tc.adapter, configContent)
			}
			if tc.adapter == "cursor" {
				for _, required := range []string{"preToolUse", "cursor-subagent-task", `"matcher": "Task"`} {
					if !strings.Contains(configContent, required) {
						t.Fatalf("Cursor subagent protocol injection is incomplete; missing %q: %s", required, configContent)
					}
				}
			}
			for _, forbidden := range []string{"guard-", "cursor-subagent-gate", `"failClosed": true`, "nativeDiscoveryTools", `tool.execute.before`, "blocked by Graphit"} {
				if strings.Contains(configContent, forbidden) {
					t.Fatalf("%s must allow native fallback; found obsolete blocker %q: %s", tc.adapter, forbidden, configContent)
				}
			}
			if tc.seed != "" && !strings.Contains(configContent, "user-token") {
				t.Fatalf("user configuration was discarded: %s", configContent)
			}

			if err := adapter.Remove(pp, map[string]map[string]string{}); err != nil {
				t.Fatal(err)
			}
			if tc.adapter == "opencode" {
				if _, err := os.Stat(target); !os.IsNotExist(err) {
					t.Fatalf("managed OpenCode plugin was not removed: %v", err)
				}
				userPlugin := filepath.Join(projectDir, ".opencode", "plugins", "user-plugin.js")
				if data, err := os.ReadFile(userPlugin); err != nil || string(data) != "user-token" {
					t.Fatalf("user OpenCode plugin changed: %q, %v", data, err)
				}
				return
			}
			remaining, err := os.ReadFile(target)
			if err != nil {
				t.Fatal(err)
			}
			if tc.seed != "" && !strings.Contains(string(remaining), "user-token") {
				t.Fatalf("user configuration was removed: %s", remaining)
			}
			if strings.Contains(string(remaining), "graphit_memory_mandatory") || strings.Contains(string(remaining), "_session-hook") {
				t.Fatalf("managed hook remained after removal: %s", remaining)
			}
		})
	}
}

func TestHookReconciliationRejectsInvalidJSONWithoutChangingIt(t *testing.T) {
	t.Parallel()

	projectDir := t.TempDir()
	target := filepath.Join(projectDir, ".cursor", "hooks.json")
	if err := os.MkdirAll(filepath.Dir(target), 0o755); err != nil {
		t.Fatal(err)
	}
	original := []byte(`{"hooks":`)
	if err := os.WriteFile(target, original, 0o644); err != nil {
		t.Fatal(err)
	}
	if err := NewCursorAdapter().syncSessionStartHook(projectDir); err == nil {
		t.Fatal("expected invalid JSON to fail")
	}
	after, err := os.ReadFile(target)
	if err != nil {
		t.Fatal(err)
	}
	if string(after) != string(original) {
		t.Fatalf("invalid file changed: got %q, want %q", after, original)
	}
}

func TestAdapterHookReplacesLegacyCentralizedCommand(t *testing.T) {
	t.Setenv("GRAPHIT_LAUNCHER_PATH", "/opt/graphit/bin/graphit")

	projectDir := t.TempDir()
	target := filepath.Join(projectDir, ".cursor", "hooks.json")
	if err := os.MkdirAll(filepath.Dir(target), 0o755); err != nil {
		t.Fatal(err)
	}
	legacy := `{"version":1,"hooks":{"sessionStart":[{"command":"user-token"},{"command":"graphit _session-hook --adapter cursor"}]}}`
	if err := os.WriteFile(target, []byte(legacy), 0o644); err != nil {
		t.Fatal(err)
	}

	if err := NewCursorAdapter().syncSessionStartHook(projectDir); err != nil {
		t.Fatal(err)
	}
	content, err := os.ReadFile(target)
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(content), "_session-hook --adapter cursor") {
		t.Fatalf("legacy centralized command remained: %s", content)
	}
	if strings.Count(string(content), "_session-hook --format "+sessionhook.FormatAdditionalContext) != 1 {
		t.Fatalf("expected one adapter-owned format command: %s", content)
	}
	if !strings.Contains(string(content), "user-token") {
		t.Fatalf("user hook was discarded: %s", content)
	}
}

func TestAdapterSyncRemovesBlockingHooksAndKeepsNativeFallback(t *testing.T) {
	t.Setenv("GRAPHIT_LAUNCHER_PATH", "/opt/graphit/bin/graphit")

	tests := []struct {
		name string
		path string
		seed string
		sync func(string) error
	}{
		{
			name: "claude",
			path: filepath.Join(".claude", "settings.json"),
			seed: `{"hooks":{"PreToolUse":[{"matcher":"Bash|Grep|Glob","hooks":[{"type":"command","command":"graphit _session-hook --format guard-claude"}]}]}}`,
			sync: NewClaudeAdapter().syncSessionStartHook,
		},
		{
			name: "codex",
			path: filepath.Join(".codex", "hooks.json"),
			seed: `{"hooks":{"PreToolUse":[{"matcher":"Bash|Grep|Glob","hooks":[{"type":"command","command":"graphit _session-hook --format guard-claude"}]}]}}`,
			sync: NewCodexAdapter().syncSessionStartHook,
		},
		{
			name: "gemini",
			path: filepath.Join(".gemini", "settings.json"),
			seed: `{"hooks":{"BeforeTool":[{"matcher":"run_shell_command|grep_search","hooks":[{"type":"command","command":"graphit _session-hook --format guard-gemini"}]}]}}`,
			sync: NewGeminiAdapter().syncSessionStartHook,
		},
		{
			name: "cursor",
			path: filepath.Join(".cursor", "hooks.json"),
			seed: `{"version":1,"hooks":{"preToolUse":[{"command":"graphit _session-hook --format cursor-subagent-task","matcher":"Task","failClosed":true},{"command":"graphit _session-hook --format guard-cursor","matcher":"Grep|Glob","failClosed":true}],"subagentStart":[{"command":"graphit _session-hook --format cursor-subagent-gate","failClosed":true}],"beforeShellExecution":[{"command":"graphit _session-hook --format guard-cursor","failClosed":true}]}}`,
			sync: NewCursorAdapter().syncSessionStartHook,
		},
		{
			name: "kiro",
			path: filepath.Join(".kiro", "hooks", "graphit-memory.json"),
			seed: `{"version":"v1","hooks":[{"name":"graphit-native-search-guard","trigger":"PreToolUse","action":{"type":"command","command":"graphit _session-hook --format guard-kiro"}}]}`,
			sync: NewKiroAdapter().syncSessionStartHook,
		},
		{
			name: "antigravity",
			path: filepath.Join(".agents", "hooks.json"),
			seed: `{"graphit-native-search-guard":{"PreToolUse":[{"hooks":[{"type":"command","command":"graphit _session-hook --format guard-antigravity"}]}]}}`,
			sync: NewAntigravityAdapter().syncSessionStartHook,
		},
		{
			name: "opencode",
			path: filepath.Join(".opencode", "plugins", opencodeManagedHookFile),
			seed: opencodeManagedMarker + "\nconst nativeDiscoveryTools = new Set([\"grep\"])\n",
			sync: NewOpenCodeAdapter().syncSessionStartHook,
		},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			projectDir := t.TempDir()
			target := filepath.Join(projectDir, tc.path)
			if err := os.MkdirAll(filepath.Dir(target), 0o755); err != nil {
				t.Fatal(err)
			}
			if err := os.WriteFile(target, []byte(tc.seed), 0o644); err != nil {
				t.Fatal(err)
			}
			if err := tc.sync(projectDir); err != nil {
				t.Fatal(err)
			}
			content, err := os.ReadFile(target)
			if err != nil {
				t.Fatal(err)
			}
			for _, forbidden := range []string{"guard-", "cursor-subagent-gate", `"failClosed": true`, "nativeDiscoveryTools", "blocked by Graphit"} {
				if strings.Contains(string(content), forbidden) {
					t.Fatalf("obsolete blocker %q remained after sync: %s", forbidden, content)
				}
			}
			if tc.name == "cursor" && !strings.Contains(string(content), "cursor-subagent-task") {
				t.Fatalf("Cursor lost its non-blocking subagent protocol injection: %s", content)
			}
		})
	}
}
