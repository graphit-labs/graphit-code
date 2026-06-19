package main

import (
	"bytes"
	"compress/gzip"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"io"
	"io/fs"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/sysutil"
	"github.com/graphit-labs/graphit-code/internal/version"
)

func main() {
	home, err := os.UserHomeDir()
	if err != nil {
		fmt.Fprintf(os.Stderr, "Error getting home directory: %v\n", err)
		os.Exit(1)
	}

	appDir := filepath.Join(home, brand.DotDir())
	versionSafe := strings.ReplaceAll(version.Version, ":", "_")
	runtimeDir := filepath.Join(appDir, "runtime", versionSafe)

	var coreBinName string
	if runtime.GOOS == "windows" {
		coreBinName = fmt.Sprintf("%s-core.exe", brand.BinName())
	} else {
		coreBinName = fmt.Sprintf("%s-core", brand.BinName())
	}

	coreBinPath := filepath.Join(runtimeDir, coreBinName)

	shouldExtract := false
	if _, err := os.Stat(coreBinPath); os.IsNotExist(err) {
		shouldExtract = true
	} else if versionSafe == "dev" {
		shouldExtract = devStampChanged(appDir)
	}

	if shouldExtract {

		cleanupOldRuntimes(filepath.Join(appDir, "runtime"), versionSafe)

		_ = os.RemoveAll(runtimeDir)

		if err := extractRuntime(runtimeDir); err != nil {
			fmt.Fprintf(os.Stderr, "Error extracting runtime: %v\n", err)
			os.Exit(1)
		}

		writeLauncherStamp(appDir)


		deduplicateModels(runtimeDir)

	}

	cmd := exec.Command(coreBinPath, os.Args[1:]...)
	cmd.Stdin = os.Stdin
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr

	env := os.Environ()

	var pathEnv string
	switch runtime.GOOS {
	case "windows":
		pathEnv = "PATH"
	case "darwin":
		pathEnv = "DYLD_LIBRARY_PATH"
	default:
		pathEnv = "LD_LIBRARY_PATH"
	}

	found := false
	for i, e := range env {
		if strings.HasPrefix(strings.ToUpper(e), strings.ToUpper(pathEnv)+"=") {
			parts := strings.SplitN(e, "=", 2)
			sep := string(os.PathListSeparator)
			env[i] = fmt.Sprintf("%s=%s%s%s", parts[0], runtimeDir, sep, parts[1])
			found = true
			break
		}
	}
	if !found {
		env = append(env, fmt.Sprintf("%s=%s", pathEnv, runtimeDir))
	}
	cmd.Env = env

	launcherExe, _ := os.Executable()
	if launcherExe != "" {
		if eval, err := filepath.EvalSymlinks(launcherExe); err == nil {
			launcherExe = eval
		}
		env = append(env, fmt.Sprintf("%s=%s", brand.EnvVar("LAUNCHER_PATH"), launcherExe))
		cmd.Env = env
	}

	sysutil.SanitizeInheritedFDs()

	var coreArgs []string
	if isMCPStdio() {
		var mcpBinName string
		if runtime.GOOS == "windows" {
			mcpBinName = fmt.Sprintf("%s-mcp.exe", brand.BinName())
		} else {
			mcpBinName = fmt.Sprintf("%s-mcp", brand.BinName())
		}
		mcpBinPath := filepath.Join(runtimeDir, mcpBinName)
		if _, statErr := os.Stat(mcpBinPath); statErr == nil {
			coreBinPath = mcpBinPath
		}
	} else {
		coreArgs = os.Args[1:]
	}

	argv := append([]string{coreBinPath}, coreArgs...)
	if err := sysutil.ReplaceProcess(coreBinPath, argv, env); err != nil {
		fmt.Fprintf(os.Stderr, "Error replacing process with core binary: %v\n", err)
		os.Exit(1)
	}
}

func extractRuntime(runtimeDir string) error {
	if err := os.MkdirAll(runtimeDir, 0755); err != nil {
		return err
	}

	return fs.WalkDir(embeddedRuntime, "runtime", func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() {
			return nil
		}

		relPath, err := filepath.Rel("runtime", path)
		if err != nil {
			return err
		}

		destPath := filepath.Join(runtimeDir, relPath)
		if err := os.MkdirAll(filepath.Dir(destPath), 0755); err != nil {
			return err
		}

		data, err := embeddedRuntime.ReadFile(path)
		if err != nil {
			return err
		}

		if strings.HasSuffix(destPath, ".gz") {
			destPath = strings.TrimSuffix(destPath, ".gz")

			gr, gzErr := gzip.NewReader(bytes.NewReader(data))
			if gzErr != nil {
				return fmt.Errorf("gzip open %s: %w", relPath, gzErr)
			}
			defer func() { _ = gr.Close() }()

			f, fErr := os.OpenFile(destPath, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, 0755)
			if fErr != nil {
				return fErr
			}
			if _, cpErr := io.Copy(f, gr); cpErr != nil {
				_ = f.Close()
				_ = os.Remove(destPath)
				return fmt.Errorf("gzip decompress %s: %w", relPath, cpErr)
			}
			return f.Close()
		}

		if err := os.WriteFile(destPath, data, 0755); err != nil {
			return err
		}

		return nil
	})
}

func cleanupOldRuntimes(runtimeBaseDir, currentVersion string) {
	entries, err := os.ReadDir(runtimeBaseDir)
	if err != nil {
		return
	}

	for _, entry := range entries {
		if entry.IsDir() && entry.Name() != currentVersion {
			oldPath := filepath.Join(runtimeBaseDir, entry.Name())

			_ = os.RemoveAll(oldPath)
		}
	}
}

func computeBuildIDStamp() string {
	h := sha256.Sum256([]byte(version.BuildID))
	return hex.EncodeToString(h[:])
}

func writeLauncherStamp(appDir string) {
	stamp := computeBuildIDStamp()
	stampDir := filepath.Join(appDir, "daemon")
	_ = os.MkdirAll(stampDir, 0o755)
	_ = os.WriteFile(filepath.Join(stampDir, "launcher.stamp"), []byte(stamp+"\n"), 0o644)
}

func isMCPStdio() bool {
	args := os.Args[1:]
	if len(args) < 2 {
		return false
	}
	return args[0] == "mcp" && args[1] == "--stdio"
}

func devStampChanged(appDir string) bool {
	stampPath := filepath.Join(appDir, "daemon", "launcher.stamp")
	existing, err := os.ReadFile(stampPath)
	if err != nil {
		return true
	}
	return strings.TrimSpace(string(existing)) != computeBuildIDStamp()
}
