package main

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"time"

	"github.com/modelcontextprotocol/go-sdk/mcp"
)

const coreSmokeProjectID = "01M00000000000000000000009"

// runCoreSmoke exercises watcher rescan and incremental jobs in an isolated
// real daemon, then the same behavior in the Linux container. No resident
// daemon or user registry is touched. The invocation is portable across hosts.
func runCoreSmoke(binary, image string, out io.Writer) error {
	ctx, cancel := context.WithTimeout(context.Background(), 4*time.Minute)
	defer cancel()
	if err := smokeNativeCore(ctx, binary, out); err != nil {
		return err
	}
	return smokeContainerCore(ctx, image, out)
}

func coreSmokeFixture(root, targetProject string) (string, string, error) {
	project, global := filepath.Join(root, "project"), filepath.Join(root, "global")
	for _, dir := range []string{project, global} {
		if err := os.MkdirAll(dir, 0700); err != nil {
			return "", "", err
		}
	}
	if targetProject == "" {
		targetProject = project
	}
	lock := `{"project":{"id":"` + coreSmokeProjectID + `","name":"core-smoke"},"config":{"modules":{"embedding":false,"memory":false,"task":false,"agent":false,"hooks":false}}}`
	if err := os.WriteFile(filepath.Join(project, "graphit.lock.json"), []byte(lock), 0600); err != nil {
		return "", "", err
	}
	registry := map[string]any{"version": 2, "projects": map[string]any{
		coreSmokeProjectID: map[string]any{"instances": []any{map[string]any{"dir": targetProject, "registeredAt": time.Now().UTC().Format(time.RFC3339)}}},
	}, "artifacts": map[string]any{}}
	data, err := json.Marshal(registry)
	if err != nil {
		return "", "", err
	}
	if err := os.WriteFile(filepath.Join(global, "global.lock.json"), data, 0600); err != nil {
		return "", "", err
	}
	if err := coreSmokeChange(project, "Before"); err != nil {
		return "", "", err
	}
	return project, global, nil
}

func coreSmokeChange(project, phase string) error {
	if err := os.WriteFile(filepath.Join(project, "main.go"), []byte("package smoke\nfunc CoreSmoke"+phase+"() {}\n"), 0600); err != nil {
		return err
	}
	return os.WriteFile(filepath.Join(project, "README.md"), []byte("# Core smoke\n\nCoreSmokeDocumentation"+phase+".\n"), 0600)
}

func coreSmokeEnv(global string) []string {
	var env []string
	for _, v := range os.Environ() {
		if !strings.HasPrefix(v, "GRAPHIT_") {
			env = append(env, v)
		}
	}
	return append(env, "GRAPHIT_GLOBAL_DIR="+global, "GRAPHIT_MODULES_DAEMON=false",
		"GRAPHIT_MODULES_DAEMON_UI=false", "GRAPHIT_MODULES_AGENT=false",
		"GRAPHIT_MODULES_MEMORY=false", "GRAPHIT_MODULES_TASK=false", "GRAPHIT_MODULES_HOOKS=false",
		"GRAPHIT_MODULES_EMBEDDING=false", "GRAPHIT_MCP_HOST=127.0.0.1", "GRAPHIT_MCP_PORT=0",
		"GRAPHIT_MAX_WORKERS=2", "GRAPHIT_DB_THREADS=2", "GRAPHIT_DB_BUFFER_MB=256")
}

func smokeNativeCore(ctx context.Context, binary string, out io.Writer) (result error) {
	exe, err := exec.LookPath(binary)
	if err != nil {
		return err
	}
	exe, err = filepath.Abs(exe)
	if err != nil {
		return err
	}
	root, err := os.MkdirTemp("", "graphit-core-smoke-")
	if err != nil {
		return err
	}
	defer os.RemoveAll(root)
	project, global, err := coreSmokeFixture(root, "")
	if err != nil {
		return err
	}
	logPath := filepath.Join(root, "daemon.log")
	log, err := os.Create(logPath)
	if err != nil {
		return err
	}
	defer log.Close()
	defer func() {
		if result != nil {
			if data, err := os.ReadFile(logPath); err == nil {
				fmt.Fprintln(out, string(data))
			}
		}
	}()
	env := coreSmokeEnv(global)
	cmd := exec.CommandContext(ctx, exe, "daemon", "--managed", "--no-embedding", "--no-dream")
	cmd.Dir, cmd.Env, cmd.Stdout, cmd.Stderr = global, env, log, log
	if err := cmd.Start(); err != nil {
		return err
	}
	done := make(chan error, 1)
	go func() { done <- cmd.Wait() }()
	waited := false
	defer func() {
		if !waited {
			_ = cmd.Process.Kill()
			<-done
		}
	}()
	address := ""
	if err := awaitSmoke(ctx, func() bool {
		data, err := os.ReadFile(filepath.Join(global, "daemon", "mcp.port"))
		address = "http://127.0.0.1:" + strings.TrimSpace(string(data))
		return err == nil && smokeHTTP(address+"/health", http.StatusOK)
	}); err != nil {
		return fmt.Errorf("core native readiness: %w", err)
	}
	key, err := os.ReadFile(filepath.Join(global, "daemon", "mcp.key"))
	if err != nil {
		return err
	}
	if err := coreSmokeJobs(ctx, address, string(key), project, project); err != nil {
		return err
	}
	stop := exec.CommandContext(ctx, exe, "daemon", "stop")
	stop.Dir, stop.Env = global, env
	if data, err := stop.CombinedOutput(); err != nil {
		return fmt.Errorf("core native stop: %w: %s", err, data)
	}
	select {
	case err := <-done:
		waited = true
		if err != nil {
			return err
		}
	case <-ctx.Done():
		return ctx.Err()
	}
	fmt.Fprintln(out, "core-smoke: host watcher rescan/incremental AST+Knowledge and joined shutdown passed")
	return nil
}

func smokeContainerCore(ctx context.Context, image string, out io.Writer) (result error) {
	root, err := os.MkdirTemp("", "graphit-core-container-smoke-")
	if err != nil {
		return err
	}
	defer os.RemoveAll(root)
	project, _, err := coreSmokeFixture(root, "/smoke/project")
	if err != nil {
		return err
	}
	name := "graphit-core-smoke-" + strconv.FormatInt(time.Now().UnixNano(), 36)
	// Root is confined to this disposable fixture: it makes private host fixture
	// permissions independent of the image user and host UID.
	args := []string{"run", "-d", "--name", name, "--user", "0:0", "--stop-timeout", "15",
		"--mount", "type=bind,src=" + root + ",dst=/smoke", "-p", "127.0.0.1::8081",
		"-e", "GRAPHIT_GLOBAL_DIR=/smoke/global", "-e", "GRAPHIT_MODULES_DAEMON_UI=false",
		"-e", "GRAPHIT_MODULES_AGENT=false", "-e", "GRAPHIT_MODULES_MEMORY=false",
		"-e", "GRAPHIT_MODULES_TASK=false", "-e", "GRAPHIT_MODULES_HOOKS=false",
		"-e", "GRAPHIT_MODULES_EMBEDDING=false", "-e", "GRAPHIT_MAX_WORKERS=2",
		"-e", "GRAPHIT_DB_THREADS=2", "-e", "GRAPHIT_DB_BUFFER_MB=256",
		image, "--no-embedding", "--no-dream"}
	if _, err := smokeDocker(ctx, args...); err != nil {
		return err
	}
	defer func() {
		if result != nil {
			if logs, err := smokeDocker(context.Background(), "logs", "--tail", "50", name); err == nil {
				fmt.Fprintln(out, logs)
			}
		}
		_, _ = smokeDocker(context.Background(), "rm", "-fv", name)
	}()
	port, err := smokeDocker(ctx, "port", name, "8081/tcp")
	if err != nil {
		return err
	}
	address := "http://" + port
	if err := awaitSmoke(ctx, func() bool { return smokeHTTP(address+"/health", http.StatusOK) }); err != nil {
		return fmt.Errorf("core container readiness: %w", err)
	}
	key, err := smokeDocker(ctx, "exec", name, "cat", "/smoke/global/daemon/mcp.key")
	if err != nil {
		return err
	}
	if err := coreSmokeJobs(ctx, address, key, project, "/smoke/project"); err != nil {
		return err
	}
	if _, err := smokeDocker(ctx, "stop", "--time", "15", name); err != nil {
		return err
	}
	exitCode, err := smokeDocker(ctx, "inspect", "--format", "{{.State.ExitCode}}", name)
	if err != nil {
		return fmt.Errorf("core container shutdown: %w", err)
	}
	if exitCode != "0" {
		return fmt.Errorf("core container shutdown exit=%s", exitCode)
	}
	fmt.Fprintln(out, "core-smoke: Docker watcher rescan/incremental AST+Knowledge and joined shutdown passed")
	return nil
}

type coreSmokeAuth struct{ key string }

func (t coreSmokeAuth) RoundTrip(req *http.Request) (*http.Response, error) {
	cloned := req.Clone(req.Context())
	cloned.Header.Set("Authorization", "Bearer "+strings.TrimSpace(t.key))
	return http.DefaultTransport.RoundTrip(cloned)
}

func coreSmokeJobs(ctx context.Context, address, key, hostProject, targetProject string) error {
	client := mcp.NewClient(&mcp.Implementation{Name: "core-smoke", Version: "1"}, nil)
	session, err := client.Connect(ctx, &mcp.StreamableClientTransport{
		Endpoint: address + "/mcp", DisableStandaloneSSE: true,
		HTTPClient: &http.Client{Transport: coreSmokeAuth{key}, Timeout: 10 * time.Second},
	}, nil)
	if err != nil {
		return err
	}
	defer session.Close()
	for _, phase := range []string{"Before", "After"} {
		if phase == "After" {
			if err := coreSmokeChange(hostProject, phase); err != nil {
				return err
			}
		}
		lastResult := "no response"
		err := awaitSmoke(ctx, func() bool {
			r, err := session.CallTool(ctx, &mcp.CallToolParams{Name: "graphit_ast_source", Arguments: map[string]any{
				"project_dir": targetProject, "path": "main.go",
			}})
			if err != nil || r.IsError {
				lastResult = fmt.Sprintf("AST error: %v", err)
				if r != nil {
					lastResult = fmt.Sprintf("AST result: %+v", r.Content)
				}
				return false
			}
			found := false
			for _, content := range r.Content {
				if text, ok := content.(*mcp.TextContent); ok {
					lastResult = text.Text
				}
				if text, ok := content.(*mcp.TextContent); ok && strings.Contains(text.Text, "CoreSmoke"+phase+"()") {
					found = true
				}
			}
			if !found {
				return false
			}
			r, err = session.CallTool(ctx, &mcp.CallToolParams{Name: "graphit_wiki_source", Arguments: map[string]any{
				"project_dir": targetProject, "path": "Core_smoke",
			}})
			if err != nil || r.IsError {
				lastResult = fmt.Sprintf("Knowledge error: %v", err)
				return false
			}
			for _, content := range r.Content {
				if text, ok := content.(*mcp.TextContent); ok {
					lastResult = text.Text
					if strings.Contains(text.Text, "CoreSmokeDocumentation"+phase) {
						return true
					}
				}
			}
			return false
		})
		if err != nil {
			return fmt.Errorf("core jobs %s AST source/Knowledge update: %w (%s)", phase, err, lastResult)
		}
	}
	return nil
}
