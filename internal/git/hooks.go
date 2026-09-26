package git

import (
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"runtime"
	"strconv"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

const preCommitEvent = "pre-commit"

func configuredHook() string { return configuredHookFor(preCommitEvent) }

func configuredHookFor(event string) string { return "hook." + brand.BinName() + "-" + event }

func hookBlockMarker() string { return strings.ToUpper(brand.Brand) + " HOOK" }

type HookManager struct {
	projectDir string
	hooksDir   string
	gitBinary  string
}

func NewHookManager(projectDir string) *HookManager {
	if projectDir == "" {
		projectDir, _ = os.Getwd()
	}
	return &HookManager{
		projectDir: projectDir,
		hooksDir:   filepath.Join(projectDir, ".git", "hooks"),
		gitBinary:  "git",
	}
}

var gitVersionPattern = regexp.MustCompile(`^git version (\d+)\.(\d+)(\.|$)`)

func (h *HookManager) hasConfiguredHooks() (bool, error) {
	output, err := exec.Command(h.gitBinary, "--version").Output()
	if err != nil {
		return false, fmt.Errorf("detect Git version: %w", err)
	}
	match := gitVersionPattern.FindStringSubmatch(strings.TrimSpace(string(output)))
	if match == nil {
		return false, fmt.Errorf("unrecognized Git version: %q", strings.TrimSpace(string(output)))
	}
	major, _ := strconv.Atoi(match[1])
	minor, _ := strconv.Atoi(match[2])
	return major > 2 || major == 2 && minor >= 54, nil
}

func (h *HookManager) gitConfig(args ...string) error {
	command := exec.Command(h.gitBinary, append([]string{"-C", h.projectDir, "config", "--local"}, args...)...)
	output, err := command.CombinedOutput()
	if err != nil {
		return fmt.Errorf("git config %s: %w: %s", strings.Join(args, " "), err, strings.TrimSpace(string(output)))
	}
	return nil
}

func (h *HookManager) gitConfigGet(key string) (string, error) {
	command := exec.Command(h.gitBinary, "-C", h.projectDir, "config", "--local", "--get", key)
	output, err := command.Output()
	return strings.TrimSpace(string(output)), err
}

// Install reconciles the default pre-commit hook and configured extra events.
// The legacy path deliberately stays at .git/hooks, ignoring core.hooksPath.
func (h *HookManager) Install(_ bool, extraEvents ...string) error {
	selected := map[string]bool{preCommitEvent: true}
	for _, event := range extraEvents {
		if !IsSupportedHookEvent(event) {
			return fmt.Errorf("unsupported Git hook event %q", event)
		}
		selected[event] = true
	}
	modern, err := h.hasConfiguredHooks()
	if err != nil {
		var instructions []string
		for _, event := range SupportedHookEvents {
			if selected[event] {
				instructions = append(instructions, event+":\nGit 2.54+: \n"+h.ManualInstructionsFor(event, true)+"\nOlder Git:\n"+h.ManualInstructionsFor(event, false))
			}
		}
		return fmt.Errorf("Git hooks not installed: %w\nCheck git --version and upgrade to Git 2.54 or newer when possible; then rerun %s sync. Manual integration:\n%s", err, brand.BinName(), strings.Join(instructions, "\n"))
	}
	var problems []error
	for _, event := range SupportedHookEvents {
		if selected[event] {
			if modern {
				if err := h.installConfigured(event); err != nil {
					problems = append(problems, h.installError(event, err, true))
				}
			} else {
				// A previous configured registration is inert on older Git, but
				// would make the legacy block skip its own invocation.
				if err := h.removeConfigured(event); err != nil {
					problems = append(problems, h.installError(event, err, false))
				}
				if err := h.installLegacy(event); err != nil {
					problems = append(problems, h.installError(event, err, false))
				}
			}
			continue
		}
		if err := h.removeConfigured(event); err != nil {
			problems = append(problems, err)
		}
		if err := h.removeLegacy(event); err != nil {
			problems = append(problems, err)
		}
	}
	return errors.Join(problems...)
}

func (h *HookManager) installConfigured(event string) error {
	name := configuredHookFor(event)
	if err := h.gitConfig("--replace-all", name+".command", hookCommandFor(event)); err != nil {
		return err
	}
	return h.gitConfig("--replace-all", name+".event", event)
}

func (h *HookManager) installLegacy(event string) error {
	dotGit := filepath.Join(h.projectDir, ".git")
	info, err := os.Stat(dotGit)
	if err != nil {
		return fmt.Errorf("open %s: %w", dotGit, err)
	}
	if !info.IsDir() {
		return fmt.Errorf("%s is not a directory", dotGit)
	}
	if err := os.MkdirAll(h.hooksDir, 0o755); err != nil {
		return fmt.Errorf("create hooks directory: %w", err)
	}
	path := filepath.Join(h.hooksDir, event)
	var existing []byte
	if data, err := os.ReadFile(path); err == nil {
		existing = data
		if hasNonShellShebang(data) {
			return fmt.Errorf("%s has a non-shell or unsupported-shell shebang", path)
		}
	} else if !errors.Is(err, os.ErrNotExist) {
		return fmt.Errorf("read %s: %w", path, err)
	}
	if err := h.writeLegacyHook(path, event, existing); err != nil {
		return fmt.Errorf("inject %s: %w", path, err)
	}
	if runtime.GOOS != "windows" {
		if err := os.Chmod(path, 0o755); err != nil {
			return fmt.Errorf("make %s executable: %w", path, err)
		}
		info, err := os.Stat(path)
		if err != nil {
			return fmt.Errorf("stat %s: %w", path, err)
		}
		if info.Mode()&0o111 == 0 {
			return fmt.Errorf("%s is not executable", path)
		}
	}
	return nil
}

// Place the Graphit block immediately after the shebang. A third-party hook
// may exit before its end, which would otherwise silently skip our checks.
func (h *HookManager) writeLegacyHook(path, event string, existing []byte) error {
	marker := hookBlockMarker()
	content, _ := removeOwnedHookBlock(string(existing), marker)
	shebang := "#!/usr/bin/env sh"
	if strings.HasPrefix(content, "#!") {
		if idx := strings.IndexByte(content, '\n'); idx >= 0 {
			shebang, content = content[:idx], content[idx+1:]
		} else {
			shebang, content = content, ""
		}
	}
	block := ShellBlockStyle.Start + marker + ShellBlockStyle.End + "\n" +
		legacyHookScriptFor(event) + "\n" +
		ShellBlockStyle.EndPrefix + marker + ShellBlockStyle.EndSuffix + "\n"
	return os.WriteFile(path, []byte(shebang+"\n"+block+content), 0o755)
}

func removeOwnedHookBlock(content, marker string) (string, bool) {
	start := ShellBlockStyle.Start + marker + ShellBlockStyle.End + "\n"
	end := ShellBlockStyle.EndPrefix + marker + ShellBlockStyle.EndSuffix + "\n"
	begin := strings.Index(content, start)
	if begin < 0 {
		return content, false
	}
	afterStart := begin + len(start)
	endOffset := strings.Index(content[afterStart:], end)
	if endOffset < 0 {
		return content, false
	}
	afterEnd := afterStart + endOffset + len(end)
	return content[:begin] + content[afterEnd:], true
}

func (h *HookManager) installError(event string, cause error, modern bool) error {
	if modern {
		return fmt.Errorf("%s hook not installed: %w\nManual integration:\n%s", event, cause, h.ManualInstructionsFor(event, true))
	}
	return fmt.Errorf("%s hook not installed: %w\nUpgrade to Git 2.54 or newer to use configured hook registration, then rerun %s sync. Manual integration:\n%s", event, cause, brand.BinName(), h.ManualInstructionsFor(event, false))
}

// ManualInstructions preserves the pre-commit default for older callers.
func (h *HookManager) ManualInstructions(modern bool) string {
	return h.ManualInstructionsFor(preCommitEvent, modern)
}

// ManualInstructionsFor returns one event's repository-local registration or shell block.
func (h *HookManager) ManualInstructionsFor(event string, modern bool) string {
	if modern {
		name := configuredHookFor(event)
		return fmt.Sprintf("cd %s\ngit config --local --replace-all %s.command %s\ngit config --local --replace-all %s.event %s", shellQuote(h.projectDir), name, shellQuote(hookCommandFor(event)), name, event)
	}
	path := filepath.Join(h.hooksDir, event)
	return fmt.Sprintf("Initialize Git first if .git is absent. Add this shell block to %s. If the file does not exist, start it with #!/usr/bin/env sh and make it executable with chmod +x %s. If its shebang is not POSIX-compatible shell, adapt the invocation to its language:\n%s", path, shellQuote(path), legacyHookScriptFor(event))
}

func (h *HookManager) Remove() error {
	var problems []error
	for _, event := range SupportedHookEvents {
		if err := h.removeConfigured(event); err != nil {
			problems = append(problems, err)
		}
		if err := h.removeLegacy(event); err != nil {
			problems = append(problems, err)
		}
	}
	return errors.Join(problems...)
}

func (h *HookManager) removeConfigured(event string) error {
	// Remove the event first so a partially removed configured hook cannot run.
	for _, key := range []string{configuredHookFor(event) + ".event", configuredHookFor(event) + ".command"} {
		if _, err := h.gitConfigGet(key); err == nil {
			if err := h.gitConfig("--unset-all", key); err != nil {
				return err
			}
		} else if exitErr := (*exec.ExitError)(nil); !errors.As(err, &exitErr) || exitErr.ExitCode() != 1 {
			// Exit 1 means that the key is absent. Other failures need attention.
			if _, statErr := os.Stat(filepath.Join(h.projectDir, ".git")); statErr == nil {
				return fmt.Errorf("read git config %s: %w", key, err)
			}
		}
	}
	return nil
}

func (h *HookManager) removeLegacy(event string) error {
	path := filepath.Join(h.hooksDir, event)
	if data, err := os.ReadFile(path); err == nil {
		if cleaned, owned := removeOwnedHookBlock(string(data), hookBlockMarker()); owned {
			if isShellShebangOnly(cleaned) {
				if err := os.Remove(path); err != nil {
					return fmt.Errorf("remove %s: %w", path, err)
				}
			} else {
				info, err := os.Stat(path)
				if err != nil {
					return fmt.Errorf("stat %s: %w", path, err)
				}
				if err := os.WriteFile(path, []byte(cleaned), info.Mode().Perm()); err != nil {
					return fmt.Errorf("remove %s: %w", path, err)
				}
			}
		}
	} else if !errors.Is(err, os.ErrNotExist) {
		return fmt.Errorf("read %s: %w", path, err)
	}
	return nil
}

func hookCommand() string {
	return hookCommandFor(preCommitEvent)
}

func hookCommandFor(event string) string {
	bin := binPath()
	// Git appends hook parameters to the configured command. A compound shell
	// statement would become invalid syntax; this wrapper receives them as $@.
	return fmt.Sprintf("sh -c 'if command -v %s >/dev/null 2>&1; then exec %s _git-hook %s \"$@\"; fi' graphit-hook", bin, bin, event)
}

func legacyHookScriptFor(event string) string {
	return fmt.Sprintf(`# %s %s checks
graphit_git_version=$(git --version 2>/dev/null) || graphit_git_version=
case "$graphit_git_version" in
  "git version "*)
    graphit_git_version=${graphit_git_version#git version }
    graphit_git_major=${graphit_git_version%%%%.*}
    graphit_git_minor=${graphit_git_version#*.}
    graphit_git_minor=${graphit_git_minor%%%%.*}
    case "$graphit_git_major:$graphit_git_minor" in
      *[!0-9:]*|:*) ;;
      *)
        if [ "$graphit_git_major" -lt 2 ] || { [ "$graphit_git_major" -eq 2 ] && [ "$graphit_git_minor" -lt 54 ]; }; then
          if ! git config --local --get %s.event >/dev/null 2>&1; then
            if command -v %s >/dev/null 2>&1; then
              %s _git-hook %s "$@" || exit $?
            fi
          fi
        fi
        ;;
    esac
    ;;
esac`, brand.DisplayName, event, configuredHookFor(event), binPath(), binPath(), event)
}

func shellQuote(value string) string {
	return "'" + strings.ReplaceAll(value, "'", "'\"'\"'") + "'"
}

func binPath() string {
	if runtime.GOOS == "windows" {
		return brand.BinNameWindows()
	}
	return brand.BinName()
}

func hasNonShellShebang(data []byte) bool {
	s := strings.TrimSpace(string(data))
	if !strings.HasPrefix(s, "#!") {
		return false
	}
	if idx := strings.IndexByte(s, '\n'); idx >= 0 {
		s = s[:idx]
	}
	fields := strings.Fields(strings.TrimPrefix(s, "#!"))
	if len(fields) == 0 {
		return true
	}
	interpreter := filepath.Base(fields[0])
	if interpreter == "env" {
		interpreter = ""
		for _, arg := range fields[1:] {
			if !strings.HasPrefix(arg, "-") {
				interpreter = filepath.Base(arg)
				break
			}
		}
	}
	switch interpreter {
	case "sh", "bash", "dash", "zsh", "ksh", "ash":
		return false
	default:
		return true
	}
}
