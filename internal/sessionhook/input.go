package sessionhook

import (
	"encoding/json"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// ReadInput reads the host's hook event from input. A terminal has no event to read: a
// first-invocation hook then gets the event the host would have sent, every other
// format none, instead of waiting on the keyboard.
func ReadInput(input io.Reader, format string) ([]byte, error) {
	if file, ok := input.(*os.File); ok {
		info, err := file.Stat()
		if err != nil {
			return nil, fmt.Errorf("inspecting session hook input: %w", err)
		}
		if info.Mode()&os.ModeCharDevice != 0 {
			if strings.EqualFold(format, FormatFirstInvocation) {
				return []byte(`{"invocationNum":0}`), nil
			}
			return nil, nil
		}
	}
	return io.ReadAll(input)
}

// ResolveProjectDir finds the Graphit project a hook event belongs to: the explicit
// directory when given, otherwise the event's working directory or workspace roots,
// otherwise this process's working directory.
func ResolveProjectDir(explicit string, input []byte) string {
	if explicit != "" {
		return FindProjectRoot(explicit)
	}
	var event struct {
		CWD            string   `json:"cwd"`
		WorkspaceRoots []string `json:"workspace_roots"`
		WorkspacePaths []string `json:"workspacePaths"`
	}
	candidates := make([]string, 0, 5)
	if json.Unmarshal(input, &event) == nil {
		candidates = append(candidates, event.CWD)
		candidates = append(candidates, event.WorkspaceRoots...)
		candidates = append(candidates, event.WorkspacePaths...)
	}
	if wd, err := os.Getwd(); err == nil {
		candidates = append(candidates, wd)
	}
	for _, candidate := range candidates {
		if root := FindProjectRoot(candidate); root != "" {
			return root
		}
	}
	return ""
}

// FindProjectRoot walks upward from start until it finds the Graphit lockfile.
// Git metadata is deliberately irrelevant: Graphit projects do not require Git.
func FindProjectRoot(start string) string {
	if start == "" {
		return ""
	}
	dir, err := filepath.Abs(start)
	if err != nil {
		return ""
	}
	if info, err := os.Stat(dir); err == nil && !info.IsDir() {
		dir = filepath.Dir(dir)
	}
	dir = filepath.Clean(dir)
	for {
		if _, err := os.Stat(filepath.Join(dir, brand.LockFileName())); err == nil {
			return dir
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			return ""
		}
		dir = parent
	}
}
