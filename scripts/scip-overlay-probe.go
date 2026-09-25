// scip-overlay-probe checks whether Docker can present a writable OverlayFS
// view of a read-only host bind without copying or changing the source tree.
//
// Run: go run ./scripts/scip-overlay-probe.go [-scratch /shared/scratch]
// The scratch directory must be outside the project, writable, and shared
// with Docker Desktop. By default the probe uses the OS temp directory.
package main

import (
	"bytes"
	"encoding/csv"
	"errors"
	"flag"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"time"
)

const probeImage = "graphit-scip-java:dev"

func main() {
	scratch := flag.String("scratch", "", "Docker-shared scratch directory outside the project (defaults to OS temp directory)")
	flag.Parse()
	if err := probe(*scratch); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}

func probe(scratch string) (err error) {
	base := scratch
	if base == "" {
		base = os.TempDir()
	}
	root, err := os.MkdirTemp(base, "graphit-scip-overlay-probe-")
	if err != nil {
		return err
	}
	defer cleanup(root)
	source := filepath.Join(root, "source")
	output := filepath.Join(root, "output")
	if err := os.MkdirAll(source, 0o755); err != nil {
		return err
	}
	if err := os.MkdirAll(output, 0o755); err != nil {
		return err
	}
	for name, content := range map[string]string{"existing.txt": "original", "deleted.txt": "preserved"} {
		if err := os.WriteFile(filepath.Join(source, name), []byte(content), 0o644); err != nil {
			return err
		}
	}
	if err := runCase("host-bind", source, output, filepath.Join(root, "host-upper"), ""); err != nil {
		return err
	}
	volume := fmt.Sprintf("graphit-scip-overlay-probe-%d-%d", os.Getpid(), time.Now().UnixNano())
	if result, err := docker("volume", "create", volume); err != nil {
		return fmt.Errorf("create Docker volume: %w: %s", err, result)
	}
	defer func() {
		result, cleanupErr := docker("volume", "rm", volume)
		if cleanupErr != nil {
			err = errors.Join(err, fmt.Errorf("remove Docker volume: %w: %s", cleanupErr, result))
		}
	}()
	if err := runCase("docker-volume", source, output, "", volume); err != nil {
		return err
	}
	return nil
}

func runCase(label, source, output, upper, volume string) error {
	if err := os.Remove(filepath.Join(output, "result.txt")); err != nil && !errors.Is(err, os.ErrNotExist) {
		return err
	}
	if upper != "" {
		if err := os.MkdirAll(upper, 0o755); err != nil {
			return err
		}
	}
	args := []string{"run", "--rm", "--cap-add=SYS_ADMIN", "--security-opt=apparmor=unconfined",
		"--mount", bind(source, "/lower", true),
		"--mount", bind(output, "/output", false)}
	if volume != "" {
		args = append(args, "--mount", "type=volume,src="+volume+",dst=/overlay")
	} else {
		args = append(args, "--mount", bind(upper, "/overlay", false))
	}
	uid, gid := os.Getuid(), os.Getgid()
	if runtime.GOOS != "linux" || uid < 0 || gid < 0 {
		uid, gid = 0, 0 // Docker Desktop maps host sharing inside its Linux VM.
	}
	script := fmt.Sprintf(`set -eu
mkdir -p /overlay/upper /overlay/work /workspace
chown %d:%d /overlay/upper /overlay/work
mount -t overlay overlay -o lowerdir=/lower,upperdir=/overlay/upper,workdir=/overlay/work /workspace
setpriv --no-new-privs --bounding-set=-all --reuid %d --regid %d --clear-groups /bin/sh -c '
  printf changed > /workspace/existing.txt
  printf new > /workspace/new.txt
  rm /workspace/deleted.txt
  test ! -e /workspace/deleted.txt
  cp /workspace/existing.txt /output/result.txt
'
`, uid, gid, uid, gid)
	args = append(args, "--entrypoint", "/bin/sh", probeImage, "-c", script)
	result, err := docker(args...)
	if err != nil {
		return fmt.Errorf("%s: Docker OverlayFS probe failed: %w: %s", label, err, result)
	}
	for name, want := range map[string]string{"existing.txt": "original", "deleted.txt": "preserved"} {
		got, err := os.ReadFile(filepath.Join(source, name))
		if err != nil {
			return fmt.Errorf("%s: source %s unreadable: %w", label, name, err)
		}
		if string(got) != want {
			return fmt.Errorf("%s: source %s changed", label, name)
		}
	}
	if _, err := os.Stat(filepath.Join(source, "new.txt")); err == nil {
		return fmt.Errorf("%s: new file appeared in source", label)
	} else if !errors.Is(err, os.ErrNotExist) {
		return fmt.Errorf("%s: inspect source: %w", label, err)
	}
	got, err := os.ReadFile(filepath.Join(output, "result.txt"))
	if err != nil {
		return fmt.Errorf("%s: output unreadable: %w", label, err)
	}
	if string(got) != "changed" {
		return fmt.Errorf("%s: output wrong", label)
	}
	if volume == "" {
		for name, want := range map[string]string{"existing.txt": "changed", "new.txt": "new"} {
			got, err := os.ReadFile(filepath.Join(upper, "upper", name))
			if err != nil {
				return fmt.Errorf("%s: upper %s unreadable: %w", label, name, err)
			}
			if string(got) != want {
				return fmt.Errorf("%s: upper %s wrong", label, name)
			}
		}
	} else {
		result, err := docker("run", "--rm", "--mount", "type=volume,src="+volume+",dst=/overlay",
			"--entrypoint", "/bin/sh", probeImage, "-c",
			"test \"$(cat /overlay/upper/existing.txt)\" = changed && test \"$(cat /overlay/upper/new.txt)\" = new")
		if err != nil {
			return fmt.Errorf("%s: upper layer missing changes: %w: %s", label, err, result)
		}
	}
	fmt.Printf("%s: PASS; source unchanged, edits isolated, output bind writable\n", label)
	return nil
}

func bind(source, target string, readonly bool) string {
	parts := []string{"type=bind", "src=" + source, "dst=" + target}
	if readonly {
		parts = append(parts, "readonly")
	}
	var out bytes.Buffer
	writer := csv.NewWriter(&out)
	_ = writer.Write(parts)
	writer.Flush()
	return strings.TrimSuffix(out.String(), "\n")
}

func docker(args ...string) (string, error) {
	output, err := exec.Command("docker", args...).CombinedOutput()
	return strings.TrimSpace(string(output)), err
}

func cleanup(root string) {
	// OverlayFS work directories can become root-owned even when the source is
	// owned by the invoking user. Limit cleanup to this newly-created fixture.
	command := "chmod -R u+rwx /cleanup 2>/dev/null || true"
	if uid, gid := os.Getuid(), os.Getgid(); uid >= 0 && gid >= 0 {
		command += fmt.Sprintf("; chown -R %d:%d /cleanup 2>/dev/null || true", uid, gid)
	}
	_, _ = docker("run", "--rm", "--mount", bind(root, "/cleanup", false),
		"--entrypoint", "/bin/sh", probeImage, "-c", command)
	if err := os.RemoveAll(root); err != nil {
		fmt.Fprintf(os.Stderr, "could not remove probe fixture %s: %v\n", root, err)
	}
}
