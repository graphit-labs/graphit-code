package ai

import (
	"context"
	"os"
	"path/filepath"
	"reflect"
	"runtime"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
)

func TestManagedWorkspaceInvocationForEveryCLI(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("shell fixture")
	}
	// Independent expected contracts: adding a supported CLI requires reviewing
	// its invocation instead of blindly accepting whatever specForBinary returns.
	required := map[string][]string{
		"claude": {"-p", "-", "--output-format", "stream-json"},
		"gemini": {"-p", "-", "--output-format", "stream-json"},
		"agy":    {"-p", "-", "--output-format", "stream-json"},
		"grok":   {"-p", "-"}, "cursor-agent": {"-p", "-"},
		"codex":    {"exec", "--json", "--skip-git-repo-check", "-"},
		"opencode": {"run", "--format", "json"}, "kiro-cli": {"chat", "--no-interactive", "-"},
		"copilot": {"-p", "-"}, "qwen": {"-p", "--output-format", "stream-json"},
		"kimi": {"-p", "--output-format", "stream-json"},
	}
	if len(required) != len(knownCLIs) {
		t.Fatal("update the independently reviewed CLI contract matrix")
	}
	required["custom-cli"] = []string{"-"}
	for name, want := range required {
		for _, allowTools := range []bool{false, true} {
			t.Run(name+map[bool]string{false: "/completion", true: "/agentic"}[allowTools], func(t *testing.T) {
				dir := t.TempDir()
				script := `#!/bin/sh
printf '%s\0' "$@" > "$AUDIT_DIR/args"
pwd > "$AUDIT_DIR/cwd"
printf '%s\n' "$NO_COLOR/$TERM/$TEST_MARKER" > "$AUDIT_DIR/env"
cat > "$AUDIT_DIR/stdin"
echo diagnostic >&2
printf 'answer'
exit "$TEST_EXIT"
`
				path := writeFakeCLI(t, name, script)
				c := &cliClient{executablePath: path, binaryName: name, agentArgs: []string{"--configured-agent-option"}}
				req := StreamRequest{SystemPrompt: "contract system", UserPrompt: "find selected context", WorkDir: dir, AllowNonGitWorkspace: true, AllowTools: allowTools, Env: map[string]string{"AUDIT_DIR": dir, "TEST_MARKER": "scoped", "TEST_EXIT": "0"}}
				var events []Event
				result, err := c.CompleteStream(context.Background(), req, collect(&events))
				// Line-oriented structured fallback retains its newline; the final
				// result must preserve exactly what was emitted, including whitespace.
				if err != nil || strings.TrimSpace(result.Text) != "answer" || result.Text != textOf(events) {
					t.Fatalf("result=%+v err=%v", result, err)
				}
				raw, _ := os.ReadFile(filepath.Join(dir, "args"))
				args := strings.Split(strings.TrimSuffix(string(raw), "\x00"), "\x00")
				has := func(arg string) bool {
					for _, got := range args {
						if got == arg {
							return true
						}
					}
					return false
				}
				for _, arg := range want {
					if !has(arg) {
						t.Errorf("missing %q in %q", arg, args)
					}
				}
				if has("--skip-git-repo-check") != (name == "codex") {
					t.Fatal("Codex option leaked or missing")
				}
				if has("--configured-agent-option") != allowTools {
					t.Fatal("configured agent arguments lost scope")
				}
				for _, unsafe := range []string{"--yolo", "--dangerously-skip-permissions", "--trust-all-tools", "--dangerously-bypass-approvals-and-sandbox"} {
					if has(unsafe) {
						t.Fatalf("unexpected permission bypass %s", unsafe)
					}
				}
				cwd, _ := os.ReadFile(filepath.Join(dir, "cwd"))
				if strings.TrimSpace(string(cwd)) != dir {
					t.Fatalf("wrong cwd %q", cwd)
				}
				env, _ := os.ReadFile(filepath.Join(dir, "env"))
				if string(env) != "1/dumb/scoped\n" {
					t.Fatalf("wrong env %q", env)
				}
				stdin, _ := os.ReadFile(filepath.Join(dir, "stdin"))
				input := string(stdin)
				if name == "opencode" || name == "qwen" || name == "kimi" {
					input = args[len(args)-1]
					if len(stdin) != 0 {
						t.Fatal("argument CLI received stdin")
					}
				}
				if !strings.Contains(input, req.SystemPrompt) || !strings.Contains(input, req.UserPrompt) {
					t.Fatalf("prompt missing in %q", input)
				}
				if events[len(events)-1].Kind != EventDone {
					t.Fatal("missing done")
				}
				req.Env["TEST_EXIT"] = "7"
				events = nil
				_, err = c.CompleteStream(context.Background(), req, collect(&events))
				if err == nil || !strings.Contains(err.Error(), "diagnostic") || events[len(events)-1].Kind != EventDone {
					t.Fatalf("error/stderr/done lost: %v", err)
				}
			})
		}
	}
}

func TestCodexNonGitOptInForInitialAndResumedTurns(t *testing.T) {
	path := writeFakeCLI(t, "codex", `#!/bin/sh
printf '%s\n' "$@" > "$AUDIT_ARGS"
case " $* " in *" --skip-git-repo-check "*) printf 'answer';; *) echo 'Not inside a trusted directory' >&2; exit 1;; esac
`)
	for _, session := range []string{"", "saved-native-session"} {
		for _, optIn := range []bool{false, true} {
			dir := t.TempDir()
			argsPath := filepath.Join(dir, "args")
			c := &cliClient{executablePath: path, binaryName: "codex"}
			_, err := c.CompleteStream(context.Background(), StreamRequest{UserPrompt: "q", WorkDir: dir, SessionID: session, AllowNonGitWorkspace: optIn, Env: map[string]string{"AUDIT_ARGS": argsPath}}, nil)
			if (err == nil) != optIn {
				t.Fatalf("session=%q optIn=%v err=%v", session, optIn, err)
			}
			raw, _ := os.ReadFile(argsPath)
			args := strings.Split(strings.TrimSpace(string(raw)), "\n")
			want := []string{"exec"}
			if session != "" {
				want = append(want, "resume", session)
			}
			want = append(want, "--json")
			if optIn {
				want = append(want, "--skip-git-repo-check")
			}
			want = append(want, "-")
			if !reflect.DeepEqual(args, want) {
				t.Fatalf("args=%q want=%q", args, want)
			}
		}
	}
	_, err := (&cliClient{executablePath: path, binaryName: "codex"}).CompleteStream(context.Background(), StreamRequest{AllowNonGitWorkspace: true}, nil)
	if err == nil || !strings.Contains(err.Error(), "explicit working directory") {
		t.Fatalf("implicit cwd accepted: %v", err)
	}
}

func TestNewClientForAgentUsesOnlySelectedExecutable(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("shell fixture")
	}
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	if err := config.SetGlobalConfigValue("ai.cli", "claude"); err != nil {
		t.Fatal(err)
	}
	dir := t.TempDir()
	names := map[string]string{"claude": "claude", "claude-code": "claude", "gemini": "gemini", "gemini-code": "gemini", "antigravity": "agy", "cursor": "cursor-agent", "codex": "codex", "opencode": "opencode", "kiro": "kiro-cli", "qwen": "qwen", "kimi": "kimi"}
	for _, binary := range names {
		if err := os.WriteFile(filepath.Join(dir, binary), []byte("#!/bin/sh\nexit 0\n"), 0755); err != nil {
			t.Fatal(err)
		}
	}
	t.Setenv("PATH", dir)
	for agent, binary := range names {
		client, err := NewClientForAgent(" " + strings.ToUpper(agent) + " ")
		if err != nil {
			t.Fatal(err)
		}
		if got := client.(*cliClient); got.binaryName != binary || got.executablePath != filepath.Join(dir, binary) {
			t.Fatalf("wrong binding: %+v", got)
		}
	}
	if err := os.Remove(filepath.Join(dir, "codex")); err != nil {
		t.Fatal(err)
	}
	if _, err := NewClientForAgent("codex"); err == nil || !strings.Contains(err.Error(), `CLI "codex"`) {
		t.Fatalf("missing selection fell back: %v", err)
	}
	for _, agent := range []string{"", "unknown", "grok", "copilot"} {
		if _, err := NewClientForAgent(agent); err == nil {
			t.Fatalf("unsupported Live adapter %q accepted", agent)
		}
	}
}
