package ai

import (
	"crypto/sha256"
	"fmt"
	"os"
	"path/filepath"
	"runtime"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// maxSocketPathLen is the longest Unix socket path every supported system accepts:
// sun_path holds 108 bytes on Linux and Windows and 104 on macOS, terminating NUL
// included.
const maxSocketPathLen = 103

// currentUID identifies the user a socket directory must belong to; tests replace it.
var currentUID = os.Getuid

// ModelSocketPath is where the server for task listens, and where every client dials it.
// It is the global directory's daemon/ folder whenever that path fits the system's
// socket path limit. A long global directory would make the listen fail, so it then
// moves to a short per-user directory derived from the global directory, which the
// server and its clients compute alike.
func ModelSocketPath(task ModelTask) string {
	return modelSocketPathIn(brand.GlobalDir(), socketRuntimeDir(), currentUID(), modelSocketName(task))
}

func modelSocketName(task ModelTask) string {
	if task == ModelTaskRerank {
		return "rerank.sock"
	}
	return "embed.sock"
}

func modelSocketPathIn(globalDir, runtimeDir string, uid int, name string) string {
	path := filepath.Join(globalDir, "daemon", name)
	if len(path) <= maxSocketPathLen {
		return path
	}
	sum := sha256.Sum256([]byte(fmt.Sprintf("%s\x00%d", filepath.Clean(globalDir), uid)))
	return filepath.Join(runtimeDir, fmt.Sprintf("%s-%x", brand.BinName(), sum[:6]), name)
}

// socketRuntimeDir is where a fallback socket directory is created: the per-user
// runtime directory when the system provides one, otherwise the temporary directory.
func socketRuntimeDir() string {
	if runtime.GOOS != "windows" {
		if dir := os.Getenv("XDG_RUNTIME_DIR"); filepath.IsAbs(dir) {
			return dir
		}
	}
	return os.TempDir()
}

// prepareSocketDir makes the directory of sockFile ready to listen in. The global
// directory's daemon folder is created as before; any other location is a fallback in
// a directory other users can reach, so it must be private to this user.
func prepareSocketDir(sockFile string) error {
	dir := filepath.Dir(sockFile)
	if dir == filepath.Join(brand.GlobalDir(), "daemon") {
		if err := os.MkdirAll(dir, 0o755); err != nil {
			return fmt.Errorf("create dir: %w", err)
		}
		return nil
	}
	return ensurePrivateDir(dir)
}
