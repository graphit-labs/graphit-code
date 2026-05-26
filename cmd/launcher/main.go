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

		shouldExtract = true
	}

	if shouldExtract {

		cleanupOldRuntimes(filepath.Join(appDir, "runtime"), versionSafe)

		os.RemoveAll(runtimeDir)

		if err := extractRuntime(runtimeDir); err != nil {
			fmt.Fprintf(os.Stderr, "Error extracting runtime: %v\n", err)
			os.Exit(1)
		}

		writeLauncherStamp(appDir, coreBinPath)

	}

	cmd := exec.Command(coreBinPath, os.Args[1:]...)
	cmd.Stdin = os.Stdin
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr

	env := os.Environ()

	var pathEnv string
	if runtime.GOOS == "windows" {
		pathEnv = "PATH"
	} else if runtime.GOOS == "darwin" {
		pathEnv = "DYLD_LIBRARY_PATH"
	} else {
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

	sanitizeInheritedFDs()

	if err := execCore(coreBinPath, env); err != nil {

		if err := cmd.Run(); err != nil {
			if exitError, ok := err.(*exec.ExitError); ok {
				os.Exit(exitError.ExitCode())
			}
			fmt.Fprintf(os.Stderr, "Error executing core binary: %v\n", err)
			os.Exit(1)
		}
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
			defer gr.Close()

			f, fErr := os.OpenFile(destPath, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, 0755)
			if fErr != nil {
				return fErr
			}
			if _, cpErr := io.Copy(f, gr); cpErr != nil {
				f.Close()
				os.Remove(destPath)
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

			os.RemoveAll(oldPath)
		}
	}
}

func writeLauncherStamp(appDir, coreBinPath string) {
	stampDir := filepath.Join(appDir, "daemon")
	_ = os.MkdirAll(stampDir, 0o755)

	f, err := os.Open(coreBinPath)
	if err != nil {
		return
	}
	defer f.Close()

	h := sha256.New()
	if _, err := io.Copy(h, f); err != nil {
		return
	}
	stamp := hex.EncodeToString(h.Sum(nil))

	stampPath := filepath.Join(stampDir, "launcher.stamp")
	_ = os.WriteFile(stampPath, []byte(stamp+"\n"), 0o644)
}
