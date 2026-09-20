package agent

import (
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/paths"
	"github.com/graphit-labs/graphit-code/internal/sessionhook"
)

// The reminder's own text says reads and bookkeeping need none, so a matcher
// that also fires on reads is the defect. A matcher that misses a real mutation
// is worse: it silently drops the checkpoint prompt.
func TestUnitReminderMatchersCoverMutationsAndSkipReads(t *testing.T) {
	t.Parallel()

	for name, tc := range map[string]struct {
		matcher    string
		mutating   []string
		readOnly   []string
		hostSource string
	}{
		"claude-style": {
			matcher:    claudeStyleMutatingTools,
			mutating:   []string{"Edit", "Write", "MultiEdit", "NotebookEdit", "Bash", "apply_patch"},
			readOnly:   []string{"Read", "Glob", "Grep", "WebFetch", "WebSearch", "TodoWrite"},
			hostSource: "Claude Code, Codex and Qwen",
		},
		"kimi": {
			matcher:  kimiMutatingTools,
			mutating: []string{"WriteFile", "StrReplaceFile", "Edit", "Bash"},
			readOnly: []string{"ReadFile", "Grep", "Glob"},
		},
		"gemini": {
			matcher:  geminiMutatingTools,
			mutating: []string{"write_file", "replace", "run_shell_command"},
			readOnly: []string{"read_file", "list_directory", "search_file_content", "google_web_search"},
		},
		"kiro": {
			matcher:  kiroMutatingTools,
			mutating: []string{"write", "shell"},
			readOnly: []string{"read", "web"},
		},
		"cursor": {
			matcher:  cursorMutatingTools,
			mutating: []string{"Write", "Delete", "Shell"},
			readOnly: []string{"Read", "Grep"},
		},
	} {
		t.Run(name, func(t *testing.T) {
			t.Parallel()
			pattern, err := regexp.Compile(tc.matcher)
			if err != nil {
				t.Fatalf("%s matcher is not a valid regex: %v", name, err)
			}
			for _, tool := range tc.mutating {
				if !pattern.MatchString(tool) {
					t.Errorf("%s matcher misses mutating tool %q, which drops its checkpoint reminder", name, tool)
				}
			}
			for _, tool := range tc.readOnly {
				if pattern.MatchString(tool) {
					t.Errorf("%s matcher still fires on read-only tool %q", name, tool)
				}
			}
		})
	}
}

// Graphit's own MCP mutations are the checkpointing. Reminding an agent to
// checkpoint right after it recorded progress is the noise this task removes.
func TestUnitReminderMatchersIgnoreGraphitBookkeeping(t *testing.T) {
	t.Parallel()

	for name, matcher := range map[string]string{
		"claude-style": claudeStyleMutatingTools,
		"kimi":         kimiMutatingTools,
		"gemini":       geminiMutatingTools,
		"cursor":       cursorMutatingTools,
	} {
		pattern := regexp.MustCompile(matcher)
		for _, tool := range []string{
			"mcp__graphit-code-stdio-mcp__graphit_task_search",
			"mcp__graphit-code-stdio-mcp__graphit_task_progress",
			"mcp__graphit-code-stdio-mcp__graphit_memory_search",
			"mcp__graphit-code-stdio-mcp__graphit_ast_source",
		} {
			if pattern.MatchString(tool) {
				t.Errorf("%s matcher fires on Graphit bookkeeping tool %q", name, tool)
			}
		}
	}
}

// The matcher has to survive into the file the host actually reads, and removal
// has to keep finding the hook now that the group carries an extra key.
func TestUnitReminderHookIsWrittenWithItsMatcherAndStillRemovable(t *testing.T) {
	t.Setenv("GRAPHIT_LAUNCHER_PATH", "/opt/graphit/bin/graphit")
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("GRAPHIT_GLOBAL_DIR", filepath.Join(home, ".graphit"))

	for _, tc := range []struct {
		adapter string
		path    string
		matcher string
		global  bool
	}{
		{adapter: "claude", path: filepath.Join(".claude", "settings.json"), matcher: claudeStyleMutatingTools},
		{adapter: "codex", path: filepath.Join(".codex", "hooks.json"), matcher: claudeStyleMutatingTools},
		{adapter: "qwen", path: filepath.Join(".qwen", "settings.json"), matcher: claudeStyleMutatingTools},
		{adapter: "gemini", path: filepath.Join(".gemini", "settings.json"), matcher: geminiMutatingTools},
		{adapter: "cursor", path: filepath.Join(".cursor", "hooks.json"), matcher: cursorMutatingTools},
		{adapter: "kiro", path: filepath.Join(".kiro", "hooks", "graphit-memory.json"), matcher: kiroMutatingTools},
		{adapter: "kimi", path: filepath.Join(".kimi-code", "config.toml"), matcher: kimiMutatingTools, global: true},
	} {
		t.Run(tc.adapter, func(t *testing.T) {
			projectDir := t.TempDir()
			targetRoot := projectDir
			if tc.global {
				targetRoot = home
			}
			target := filepath.Join(targetRoot, tc.path)

			adapter := GetAdapter(tc.adapter)
			pp := &paths.ProjectPaths{ActiveProjectDir: projectDir}
			if err := adapter.Sync(map[string]map[string]string{}, pp, "matcher-test-project"); err != nil {
				t.Fatal(err)
			}
			written, err := os.ReadFile(target)
			if err != nil {
				t.Fatal(err)
			}
			if !strings.Contains(string(written), tc.matcher) {
				t.Fatalf("%s did not record the mutating-tool matcher %q: %s", tc.adapter, tc.matcher, written)
			}

			if err := adapter.Remove(pp, map[string]map[string]string{}); err != nil {
				t.Fatal(err)
			}
			after, err := os.ReadFile(target)
			if os.IsNotExist(err) {
				return
			}
			if err != nil {
				t.Fatal(err)
			}
			if strings.Contains(string(after), tc.matcher) {
				t.Fatalf("%s left the matched reminder hook behind: %s", tc.adapter, after)
			}
		})
	}
}

func TestEveryAdapterInstallsOneOrderedSessionMemoryHook(t *testing.T) {
	const launcherPath = "/opt/graphit/bin/graphit"
	t.Setenv("GRAPHIT_LAUNCHER_PATH", launcherPath)
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("GRAPHIT_GLOBAL_DIR", filepath.Join(home, ".graphit"))

	tests := []struct {
		adapter string
		path    string
		seed    string
		format  string
		global  bool
	}{
		{"antigravity", filepath.Join(".agents", "hooks.json"), `{"user-hook":{"Stop":[{"command":"user-token"}]}}`, sessionhook.FormatFirstInvocation, false},
		{"cursor", filepath.Join(".cursor", "hooks.json"), `{"version":1,"hooks":{"sessionStart":[{"command":"user-token"}]}}`, sessionhook.FormatAdditionalContext, false},
		{"claude", filepath.Join(".claude", "settings.json"), `{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"user-token"}]}]}}`, sessionhook.FormatSessionStart, false},
		{"kiro", filepath.Join(".kiro", "hooks", "graphit-memory.json"), `{"version":"v1","hooks":[{"name":"user-hook","trigger":"Stop","action":{"type":"command","command":"user-token"}}]}`, "", false},
		{"codex", filepath.Join(".codex", "hooks.json"), `{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"user-token"}]}]}}`, sessionhook.FormatSessionStart, false},
		{"opencode", filepath.Join(".opencode", "plugins", opencodeManagedHookFile), "", "", false},
		{"gemini", filepath.Join(".gemini", "settings.json"), `{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"user-token"}]}]},"userSetting":true}`, sessionhook.FormatSessionStart, false},
		{"qwen", filepath.Join(".qwen", "settings.json"), `{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"user-token"}]}]},"userSetting":true}`, sessionhook.FormatSessionStart, false},
		{"kimi", filepath.Join(".kimi-code", "config.toml"), "user_setting = \"user-token\"\n", sessionhook.FormatSessionPrompt, true},
	}
	if len(tests) != len(SupportedAgents()) {
		t.Fatalf("adapter bootstrap matrix has %d entries, want one for each of %d supported Agents", len(tests), len(SupportedAgents()))
	}

	for _, tc := range tests {
		t.Run(tc.adapter, func(t *testing.T) {
			projectDir := t.TempDir()
			targetRoot := projectDir
			if tc.global {
				targetRoot, _ = os.UserHomeDir()
			}
			target := filepath.Join(targetRoot, tc.path)
			if !tc.global && !strings.HasPrefix(target, projectDir+string(os.PathSeparator)) {
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
			if strings.Contains(configContent, projectDir) {
				t.Fatalf("%s hook embeds the sync machine's checkout path: %s", tc.adapter, configContent)
			}
			if strings.Contains(configContent, "--project-dir") {
				t.Fatalf("%s hook must resolve its project from native runtime input: %s", tc.adapter, configContent)
			}
			if strings.Contains(configContent, launcherPath) {
				t.Fatalf("%s hook embeds the sync machine's executable path: %s", tc.adapter, configContent)
			}
			if tc.adapter == "opencode" && !strings.Contains(configContent, `cwd: directory, stdin: nativeInput(sessionID)`) {
				t.Fatalf("OpenCode must execute the hook from its runtime directory: %s", configContent)
			}
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
			for _, required := range []string{"graphit_task_search", "graphit_task_get", "Follow `next_cursor` only while a relevant gap remains"} {
				if !strings.Contains(protocolContent, required) {
					t.Fatalf("%s bootstrap missing Task recall requirement %q: %s", tc.adapter, required, protocolContent)
				}
			}
			for _, required := range []string{"graphit_task_session_list", "graphit_task_session_get", "graphit_task_session_create", "graphit_task_session_checkpoint", "graphit_task_session_revise", "graphit_task_session_complete", "claim only their task", "Stop hooks never close sessions"} {
				if !strings.Contains(protocolContent, required) {
					t.Fatalf("%s cannot deliver session lifecycle requirement %q", tc.adapter, required)
				}
			}
			if tc.adapter == "cursor" || tc.adapter == "antigravity" {
				for _, required := range []string{"specific hook compensation"} {
					if !strings.Contains(protocolContent, required) {
						t.Fatalf("%s gap compensation missing %q: %s", tc.adapter, required, protocolContent)
					}
				}
				if tc.adapter == "antigravity" && (!strings.Contains(protocolContent, "role document installed under the agents directory") || !strings.Contains(protocolContent, "read that document yourself when you cannot delegate")) {
					t.Fatalf("Antigravity subagent compensation does not point at the installed role document: %s", protocolContent)
				}
			}
			if !strings.Contains(protocolContent, "default native tools") {
				t.Fatalf("%s bootstrap must preserve native fallback when Graphit MCP tools are unavailable: %s", tc.adapter, protocolContent)
			}
			if tc.adapter == "opencode" && (!strings.Contains(configContent, `Bun.spawnSync`) || !strings.Contains(configContent, `experimental.chat.system.transform`) || !strings.Contains(configContent, `experimental.session.compacting`)) {
				t.Fatalf("OpenCode plugin must load memory and inject at model/compaction boundaries: %s", configContent)
			}
			if tc.adapter == "opencode" {
				for _, required := range []string{"code/documentation consistency in both directions", "acceptance checks", "Resolve divergence before closing"} {
					if !strings.Contains(configContent, required) {
						t.Fatalf("OpenCode tool checkpoint missing documentation consistency requirement %q: %s", required, configContent)
					}
				}
			}
			if tc.adapter != "opencode" && !strings.Contains(configContent, "--sync") {
				t.Fatalf("%s must install an asynchronous final-sync dispatcher: %s", tc.adapter, configContent)
			}
			if tc.adapter == "kiro" && strings.Count(configContent, "_session-hook --format plain-context") != 2 {
				t.Fatalf("Kiro must cover Agent SessionStart and CLI AgentSpawn: %s", configContent)
			}
			if tc.adapter == "gemini" && !strings.Contains(configContent, "BeforeAgent") {
				t.Fatalf("Gemini must reassert the invariant before every agent turn: %s", configContent)
			}
			if (tc.adapter == "claude" || tc.adapter == "codex" || tc.adapter == "qwen" || tc.adapter == "kimi") && !strings.Contains(configContent, "SubagentStart") {
				t.Fatalf("%s must bootstrap subagents: %s", tc.adapter, configContent)
			}
			if tc.adapter == "cursor" {
				for _, required := range []string{"preToolUse", "cursor-subagent-task", `"matcher": "Task"`, "postToolUse", "cursor-unit", "subagentStop", "stop", "cursor-stop", "sessionEnd", "session-end --sync"} {
					if !strings.Contains(configContent, required) {
						t.Fatalf("Cursor lifecycle is incomplete; missing %q: %s", required, configContent)
					}
				}
				if strings.Count(configContent, "_session-hook --format cursor-stop --sync") != 2 {
					t.Fatalf("Cursor must sync both subagent and main-agent completion: %s", configContent)
				}
			}
			if tc.adapter == "claude" || tc.adapter == "codex" || tc.adapter == "qwen" || tc.adapter == "kimi" {
				promptFormat := "user-prompt"
				if tc.adapter == "kimi" {
					// Kimi carries the bootstrap on the turn boundary because its
					// session-start output never reaches the model, so it gets the
					// session-scoped format instead of the bare invariant.
					promptFormat = "session-prompt"
				}
				for _, required := range []string{"UserPromptSubmit", promptFormat, "PostToolUse", "post-tool-use", "SubagentStop", "Stop", "SessionEnd", "session-end --sync"} {
					if !strings.Contains(configContent, required) {
						t.Fatalf("%s lifecycle is incomplete; missing %q: %s", tc.adapter, required, configContent)
					}
				}
				if strings.Count(configContent, "_session-hook --format stop --sync") != 2 {
					t.Fatalf("%s must sync both subagent and main-agent completion: %s", tc.adapter, configContent)
				}
			}
			if tc.adapter == "gemini" {
				for _, required := range []string{"AfterTool", "after-tool", "AfterAgent", "after-agent --sync", "SessionEnd", "session-end --sync"} {
					if !strings.Contains(configContent, required) {
						t.Fatalf("Gemini lifecycle is incomplete; missing %q: %s", required, configContent)
					}
				}
			}
			if tc.adapter == "kiro" {
				for _, required := range []string{"UserPromptSubmit", "PostToolUse", "PostTaskExec", "Stop", "plain-unit", "no-output --sync", "after meaningful work"} {
					if !strings.Contains(configContent, required) {
						t.Fatalf("Kiro lifecycle is incomplete; missing %q: %s", required, configContent)
					}
				}
			}
			if tc.adapter == "antigravity" {
				for _, required := range []string{"PreInvocation", "PostInvocation", "post-invocation", "Stop", "antigravity-stop --sync"} {
					if !strings.Contains(configContent, required) {
						t.Fatalf("Antigravity lifecycle is incomplete; missing %q: %s", required, configContent)
					}
				}
				if strings.Contains(configContent, `"PostToolUse"`) {
					t.Fatalf("Antigravity PostToolUse accepts only {}; task feedback must remain on PostInvocation: %s", configContent)
				}
			}
			if tc.adapter == "opencode" {
				for _, required := range []string{`"tool.execute.after"`, `event.type === "session.idle"`, `event.type === "session.deleted"`, `Bun.spawn([`, `subprocess.unref()`, `"no-output", "--sync"`, "after meaningful work"} {
					if !strings.Contains(configContent, required) {
						t.Fatalf("OpenCode lifecycle is incomplete; missing %q: %s", required, configContent)
					}
				}
			}
			if strings.Contains(configContent, `"timeout": 600`) {
				t.Fatalf("%s final sync must not reserve a synchronous wait timeout: %s", tc.adapter, configContent)
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

func TestHookExecutableQuotingIsPortable(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name     string
		goos     string
		argument string
		want     string
	}{
		{name: "linux spaces and apostrophe", goos: "linux", argument: "/opt/Graphit's Tools/graphit", want: `'/opt/Graphit'"'"'s Tools/graphit'`},
		{name: "macOS spaces", goos: "darwin", argument: "/Applications/Graphit Tools/graphit", want: `'/Applications/Graphit Tools/graphit'`},
		{name: "Windows spaces", goos: "windows", argument: `C:\Program Files\Graphit\graphit.exe`, want: `"C:\Program Files\Graphit\graphit.exe"`},
		{name: "Windows trailing slash", goos: "windows", argument: `C:\Graphit\`, want: `"C:\Graphit\\"`},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()
			if got := quoteHookCommandArgument(tc.goos, tc.argument); got != tc.want {
				t.Fatalf("quoted argument = %q, want %q", got, tc.want)
			}
		})
	}
}

func TestKimiGlobalHooksStayUntilLastProjectIsRemoved(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("GRAPHIT_GLOBAL_DIR", filepath.Join(home, ".graphit"))
	a := NewKimiAdapter()
	first, second := t.TempDir(), t.TempDir()
	for _, project := range []string{first, second} {
		if err := a.Sync(nil, &paths.ProjectPaths{ActiveProjectDir: project}, "project"); err != nil {
			t.Fatal(err)
		}
	}
	config := filepath.Join(home, ".kimi-code", "config.toml")
	if err := a.Remove(&paths.ProjectPaths{ActiveProjectDir: first}, nil); err != nil {
		t.Fatal(err)
	}
	data, err := os.ReadFile(config)
	if err != nil || !strings.Contains(string(data), "_session-hook") {
		t.Fatalf("shared Kimi hooks disappeared early: %q, %v", data, err)
	}
	if err := a.Remove(&paths.ProjectPaths{ActiveProjectDir: second}, nil); err != nil {
		t.Fatal(err)
	}
	if data, err = os.ReadFile(config); err == nil && strings.Contains(string(data), "_session-hook") {
		t.Fatalf("Kimi hook remained after last project: %s", data)
	}
}

func TestSessionHookCommandUsesPATHAcrossOperatingSystems(t *testing.T) {
	t.Parallel()
	tests := []struct {
		goos string
		want string
	}{
		{goos: "linux", want: `'graphit' _session-hook --format session-start`},
		{goos: "darwin", want: `'graphit' _session-hook --format session-start`},
		{goos: "windows", want: `"graphit" _session-hook --format session-start`},
	}
	for _, tc := range tests {
		t.Run(tc.goos, func(t *testing.T) {
			t.Parallel()
			if got := sessionHookCommandForOS(tc.goos, "session-start"); got != tc.want {
				t.Fatalf("session hook command = %q, want %q", got, tc.want)
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
