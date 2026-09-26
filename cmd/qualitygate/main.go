// Command qualitygate runs the project's portable Git quality gates.
package main

import (
	"bytes"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
)

type step struct {
	name    string
	command string
	args    []string
	dir     string
}

var unitPackages = []string{
	"./cmd/qualitygate/...", "./internal/git/...", "./internal/hub/adapters/agent/...",
	"./internal/brand/...", "./internal/config/...", "./internal/ignorer/...",
	"./internal/lockfile/...", "./internal/netutil/...", "./internal/output/...",
	"./internal/pagination/...", "./internal/sessionhook/...", "./internal/slogutil/...",
	"./internal/sysutil/...", "./internal/task/...", "./internal/textslice/...", "./internal/toon/...",
	"./internal/version/...",
}

func main() {
	if err := run(os.Args[1:], os.Stdout, os.Stderr); err != nil {
		fmt.Fprintln(os.Stderr, "qualitygate:", err)
		os.Exit(1)
	}
}

func run(args []string, stdout, stderr io.Writer) error {
	if len(args) != 1 || (args[0] != "pre-commit" && args[0] != "pre-push") {
		return fmt.Errorf("usage: go run ./cmd/qualitygate <pre-commit|pre-push>")
	}
	if _, err := os.Stat("graphit.lock.json"); err != nil {
		return fmt.Errorf("run from the project root: %w", err)
	}
	if args[0] == "pre-commit" {
		files, err := trackedGoFiles()
		if err != nil {
			return err
		}
		if err := checkFormatting(files, stdout, stderr); err != nil {
			return err
		}
		workflows, err := workflowFiles()
		if err != nil {
			return err
		}
		return runSteps(preCommitSteps(workflows), stdout, stderr)
	}
	return runSteps(prePushSteps(), stdout, stderr)
}

func preCommitSteps(workflows []string) []step {
	return []step{
		{name: "Go unit tests", command: "go", args: append([]string{"test", "-p", "1", "-parallel", "2", "-timeout", "2m"}, unitPackages...)},
		{name: "Go lint and SAST (gosec)", command: "golangci-lint", args: []string{"run", "./..."}},
		{name: "GitHub Actions lint", command: "go", args: append([]string{"run", "github.com/rhysd/actionlint/cmd/actionlint@v1.7.7", "-no-color"}, workflows...)},
		{name: "UI lint", command: "npm", args: []string{"run", "lint"}, dir: "internal/ui"},
		{name: "UI tests", command: "npm", args: []string{"test"}, dir: "internal/ui"},
		{name: "UI typecheck and build", command: "npm", args: []string{"run", "build"}, dir: "internal/ui"},
	}
}

func prePushSteps() []step {
	return []step{
		{name: "Go vulnerability analysis", command: "go", args: []string{"run", "golang.org/x/vuln/cmd/govulncheck@v1.7.0", "-tags", "lancedb", "./..."}},
	}
}

func workflowFiles() ([]string, error) {
	workflows, err := filepath.Glob(filepath.Join(".github", "workflows", "*.yml"))
	if err != nil {
		return nil, fmt.Errorf("find workflow files: %w", err)
	}
	if len(workflows) == 0 {
		return nil, fmt.Errorf("no workflow files found")
	}
	return workflows, nil
}

func trackedGoFiles() ([]string, error) {
	cmd := exec.Command("git", "ls-files", "-z", "--cached", "--", "*.go")
	output, err := cmd.Output()
	if err != nil {
		return nil, fmt.Errorf("list Go files: %w", err)
	}
	var files []string
	for _, file := range bytes.Split(output, []byte{0}) {
		if len(file) == 0 {
			continue
		}
		if _, err := os.Stat(string(file)); err == nil {
			files = append(files, string(file))
		}
	}
	return files, nil
}

func checkFormatting(files []string, stdout, stderr io.Writer) error {
	if len(files) == 0 {
		return fmt.Errorf("no Go files found for formatting check")
	}
	fmt.Fprintln(stdout, "qualitygate: Go formatting")
	cmd := exec.Command("gofmt", append([]string{"-l"}, files...)...)
	var unformatted bytes.Buffer
	cmd.Stdout = io.MultiWriter(stdout, &unformatted)
	cmd.Stderr = stderr
	if err := cmd.Run(); err != nil {
		return fmt.Errorf("go formatting: %w", err)
	}
	if unformatted.Len() != 0 {
		return fmt.Errorf("go formatting: run gofmt on the listed files")
	}
	return nil
}

func runSteps(steps []step, stdout, stderr io.Writer) error {
	for _, s := range steps {
		fmt.Fprintln(stdout, "qualitygate:", s.name)
		cmd := exec.Command(s.command, s.args...)
		cmd.Dir = s.dir
		cmd.Stdout, cmd.Stderr = stdout, stderr
		if err := cmd.Run(); err != nil {
			var pathErr *exec.Error
			if errors.As(err, &pathErr) {
				return fmt.Errorf("%s: %s is unavailable on PATH: %w", s.name, s.command, err)
			}
			return fmt.Errorf("%s: %w", s.name, err)
		}
	}
	return nil
}
