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
	"regexp"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/graphit-labs/graphit-code/internal/daemonctl"
)

// runUISmoke uses an already-built host launcher and container image. Its Go
// invocation is the same on Windows, macOS and Linux; Docker must be available.
func runUISmoke(binary, image string, out io.Writer) error {
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Minute)
	defer cancel()
	if err := smokeNativeDaemonUI(ctx, binary, out); err != nil {
		return err
	}
	if err := smokeStandaloneUI(ctx, binary, out); err != nil {
		return err
	}
	for _, tc := range []struct{ ui, auth bool }{{false, true}, {true, false}, {true, true}} {
		if err := smokeContainerUI(ctx, image, tc.ui, tc.auth, out); err != nil {
			return err
		}
	}
	return nil
}

func smokeNativeDaemonUI(ctx context.Context, binary string, out io.Writer) error {
	exe, err := exec.LookPath(binary)
	if err != nil {
		return err
	}
	exe, err = filepath.Abs(exe)
	if err != nil {
		return err
	}
	dir, err := os.MkdirTemp("", "graphit-daemon-ui-smoke-")
	if err != nil {
		return err
	}
	defer os.RemoveAll(dir)
	var env []string
	for _, v := range os.Environ() {
		if !strings.HasPrefix(v, "GRAPHIT_") {
			env = append(env, v)
		}
	}
	env = append(env, "GRAPHIT_GLOBAL_DIR="+dir, "GRAPHIT_MODULES_DAEMON=false", "GRAPHIT_MODULES_DAEMON_UI=", "GRAPHIT_MODULES_AGENT=false", "GRAPHIT_MODULES_MEMORY=false", "GRAPHIT_UI_AUTH_ENABLED=false", "GRAPHIT_MCP_HOST=127.0.0.1", "GRAPHIT_MCP_PORT=0", "GRAPHIT_UI_HOST=127.0.0.1", "GRAPHIT_MAX_WORKERS=2")
	// Stored true must not activate the UI without the environment opt-in.
	cfg := exec.CommandContext(ctx, exe, "config", "--global", "modules.daemon_ui", "true")
	cfg.Dir = dir
	cfg.Env = env
	if data, err := cfg.CombinedOutput(); err != nil {
		return fmt.Errorf("native stored config: %w: %s", err, data)
	}
	log, err := os.Create(filepath.Join(dir, "daemon.log"))
	if err != nil {
		return err
	}
	defer log.Close()
	cmd := exec.CommandContext(ctx, exe, "daemon", "--managed", "--no-embedding", "--no-dream")
	cmd.Dir = dir
	cmd.Env = env
	cmd.Stdout = log
	cmd.Stderr = log
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
	control := ""
	if err := awaitSmoke(ctx, func() bool {
		data, e := os.ReadFile(filepath.Join(dir, "daemon", "mcp.port"))
		control = "http://127.0.0.1:" + strings.TrimSpace(string(data))
		return e == nil && smokeHTTP(control+"/health", http.StatusOK)
	}); err != nil {
		return fmt.Errorf("native daemon readiness: %w", err)
	}
	pidPath := filepath.Join(dir, "daemon", "daemon.pid")
	initialPID, err := os.ReadFile(pidPath)
	if err != nil {
		return err
	}
	actualPID := strings.SplitN(strings.TrimSpace(string(initialPID)), "\n", 2)[0]
	urlPath := filepath.Join(dir, "daemon", "ui.url")
	if _, err := os.Stat(urlPath); !os.IsNotExist(err) {
		return fmt.Errorf("managed daemon activated UI without environment")
	}
	key, err := os.ReadFile(filepath.Join(dir, "daemon", "mcp.key"))
	if err != nil {
		return err
	}
	if err := smokeConcurrentActivation(ctx, control, strings.TrimSpace(string(key)), ""); err != nil {
		return err
	}
	owner, err := os.ReadFile(urlPath)
	if err != nil {
		return err
	}
	lines := strings.Split(strings.TrimSpace(string(owner)), "\n")
	if len(lines) != 2 || lines[0] != actualPID {
		return fmt.Errorf("native UI owner changed daemon PID")
	}
	if !smokeHTTP(lines[1]+"health", http.StatusOK) {
		return fmt.Errorf("native UI not bound")
	}
	cli := exec.CommandContext(ctx, exe, "ui", "--non-interactive")
	cli.Dir = dir
	cli.Env = env
	if data, err := cli.CombinedOutput(); err != nil {
		return fmt.Errorf("native UI command: %w: %s", err, data)
	}
	current, err := os.ReadFile(filepath.Join(dir, "daemon", "daemon.pid"))
	if err != nil {
		return err
	}
	if string(current) != string(initialPID) {
		return fmt.Errorf("UI request replaced native daemon")
	}
	stop := exec.CommandContext(ctx, exe, "daemon", "stop")
	stop.Env = env
	stop.Dir = dir
	if data, err := stop.CombinedOutput(); err != nil {
		return fmt.Errorf("native daemon stop: %w: %s", err, data)
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
	if _, err := os.Stat(urlPath); !os.IsNotExist(err) {
		return fmt.Errorf("native UI publication survived stop")
	}
	if smokeHTTP(lines[1]+"health", http.StatusOK) {
		return fmt.Errorf("native UI listener survived daemon stop")
	}
	fmt.Fprintln(out, "ui-smoke: native managed env absent/stored true, concurrent activation, CLI/PID reuse and stop passed")
	return nil
}

func smokeConcurrentActivation(ctx context.Context, control, key, want string) error {
	var wg sync.WaitGroup
	answers := make(chan struct {
		url string
		err error
	}, 8)
	for range 8 {
		wg.Add(1)
		go func() {
			defer wg.Done()
			req, err := http.NewRequestWithContext(ctx, http.MethodPost, control+daemonctl.UIStartPath, nil)
			if err != nil {
				answers <- struct {
					url string
					err error
				}{"", err}
				return
			}
			req.Header.Set("Authorization", "Bearer "+key)
			client := http.Client{Timeout: 35 * time.Second}
			resp, err := client.Do(req)
			if err != nil {
				answers <- struct {
					url string
					err error
				}{"", err}
				return
			}
			defer resp.Body.Close()
			var body struct {
				URL string `json:"url"`
			}
			err = json.NewDecoder(io.LimitReader(resp.Body, 4096)).Decode(&body)
			if resp.StatusCode != http.StatusOK {
				err = fmt.Errorf("activation HTTP %d", resp.StatusCode)
			}
			answers <- struct {
				url string
				err error
			}{body.URL, err}
		}()
	}
	wg.Wait()
	close(answers)
	for answer := range answers {
		if answer.err != nil {
			return answer.err
		}
		if answer.url == "" {
			return fmt.Errorf("empty activation URL")
		}
		if want == "" {
			want = answer.url
		}
		if answer.url != want {
			return fmt.Errorf("concurrent requests selected different UI URLs")
		}
	}
	return nil
}

func smokeMemory(ctx context.Context, name, stage string, out io.Writer) {
	data, err := smokeDocker(ctx, "exec", name, "cat", "/proc/1/smaps_rollup")
	if err != nil {
		return
	}
	for _, line := range strings.Split(data, "\n") {
		if strings.HasPrefix(line, "Rss:") || strings.HasPrefix(line, "Pss:") {
			fmt.Fprintf(out, "ui-smoke: isolated daemon %s %s\n", stage, line)
		}
	}
}

func smokeStandaloneUI(ctx context.Context, binary string, out io.Writer) error {
	exe, err := exec.LookPath(binary)
	if err != nil {
		return fmt.Errorf("host launcher: %w", err)
	}
	exe, err = filepath.Abs(exe)
	if err != nil {
		return err
	}
	dir, err := os.MkdirTemp("", "graphit-ui-smoke-")
	if err != nil {
		return err
	}
	defer os.RemoveAll(dir)
	log, err := os.Create(filepath.Join(dir, "ui.log"))
	if err != nil {
		return err
	}
	defer log.Close()
	cmd := exec.CommandContext(ctx, exe, "ui", "--repo", dir, "--non-interactive")
	cmd.Dir = dir
	for _, value := range os.Environ() {
		if !strings.HasPrefix(value, "GRAPHIT_") {
			cmd.Env = append(cmd.Env, value)
		}
	}
	cmd.Env = append(cmd.Env, "GRAPHIT_GLOBAL_DIR="+dir, "GRAPHIT_SUBPROCESS=1", "GRAPHIT_BACKGROUND=1", "GRAPHIT_PARENT_PIPE=1", "GRAPHIT_MODULES_AGENT=false", "GRAPHIT_MODULES_DAEMON=false", "GRAPHIT_MODULES_EMBEDDING=false", "GRAPHIT_UI_AUTH_ENABLED=false", "GRAPHIT_UI_HOST=127.0.0.1", "GRAPHIT_UI_PORT=8080")
	pipe, err := cmd.StdinPipe()
	if err != nil {
		return err
	}
	cmd.Stdout, cmd.Stderr = log, log
	if err := cmd.Start(); err != nil {
		_ = pipe.Close()
		return err
	}
	done := make(chan error, 1)
	go func() { done <- cmd.Wait() }()
	waited := false
	defer func() {
		_ = pipe.Close()
		if !waited {
			_ = cmd.Process.Kill()
			<-done
		}
	}()
	var address string
	if err := awaitSmoke(ctx, func() bool {
		data, e := os.ReadFile(filepath.Join(dir, "ui.log"))
		address = regexp.MustCompile(`http://127\.0\.0\.1:[0-9]+/`).FindString(string(data))
		return e == nil && address != ""
	}); err != nil {
		return fmt.Errorf("native UI readiness: %w", err)
	}
	if !smokeHTTP(address+"health", http.StatusOK) {
		return fmt.Errorf("native UI address is not healthy")
	}
	fmt.Fprintln(out, "ui-smoke: host UI bound", address)
	// EOF is the exact signal used when the daemon dies, independent of OS signals.
	_ = pipe.Close()
	select {
	case err := <-done:
		waited = true
		if err != nil {
			return fmt.Errorf("UI parent-pipe shutdown: %w", err)
		}
	case <-time.After(15 * time.Second):
		return fmt.Errorf("UI survived parent-pipe EOF")
	case <-ctx.Done():
		return ctx.Err()
	}
	if smokeHTTP(address+"health", http.StatusOK) {
		return fmt.Errorf("UI listener survived shutdown")
	}
	fmt.Fprintln(out, "ui-smoke: host UI parent-death shutdown passed")
	return nil
}

func smokeContainerUI(ctx context.Context, image string, enabled, auth bool, out io.Writer) (result error) {
	name := "graphit-ui-smoke-" + strconv.FormatInt(time.Now().UnixNano(), 36)
	args := []string{"run", "-d", "--name", name, "--stop-timeout", "10", "-p", "127.0.0.1::8080", "-p", "127.0.0.1::8081", "-e", "GRAPHIT_MODULES_DAEMON_UI=" + strconv.FormatBool(enabled), "-e", "GRAPHIT_UI_AUTH_ENABLED=" + strconv.FormatBool(auth), "-e", "GRAPHIT_MODULES_EMBEDDING=false", "-e", "GRAPHIT_MAX_WORKERS=2", "-e", "GRAPHIT_DB_THREADS=2", "-e", "GRAPHIT_DB_BUFFER_MB=256", image, "--no-embedding", "--no-dream"}
	if _, err := smokeDocker(ctx, args...); err != nil {
		return err
	}
	defer func() {
		if result != nil {
			if logs, err := smokeDocker(context.Background(), "logs", "--tail", "30", name); err == nil {
				fmt.Fprintln(out, logs)
			}
		}
		_, _ = smokeDocker(context.Background(), "rm", "-fv", name)
	}()
	port, err := smokeDocker(ctx, "port", name, "8081/tcp")
	if err != nil {
		return err
	}
	if err := awaitSmoke(ctx, func() bool { return smokeHTTP("http://"+port+"/health", http.StatusOK) }); err != nil {
		return fmt.Errorf("container MCP health: %w", err)
	}
	uiPort, err := smokeDocker(ctx, "port", name, "8080/tcp")
	if err != nil {
		return err
	}
	uiAddress := "http://" + uiPort + "/"
	if !enabled {
		if smokeHTTP(uiAddress+"health", http.StatusOK) {
			return fmt.Errorf("unrequested container UI is listening")
		}
		if _, err := smokeDocker(ctx, "exec", name, "test", "!", "-e", "/home/graphit/.graphit/daemon/ui.url"); err != nil {
			return fmt.Errorf("unrequested container published UI URL: %w", err)
		}
		smokeMemory(ctx, name, "before activation", out)
		key, err := smokeDocker(ctx, "exec", name, "cat", "/home/graphit/.graphit/daemon/mcp.key")
		if err != nil {
			return err
		}
		if err := smokeConcurrentActivation(ctx, "http://"+port, key, "http://127.0.0.1:8080/"); err != nil {
			return err
		}
		// Exercise the same default command as a user, reusing foreground PID 1.
		if _, err := smokeDocker(ctx, "exec", name, "graphit", "ui", "--non-interactive"); err != nil {
			return err
		}
		smokeMemory(ctx, name, "after activation (same daemon)", out)
	}
	if err := awaitSmoke(ctx, func() bool { return smokeHTTP(uiAddress+"health", http.StatusOK) }); err != nil {
		return fmt.Errorf("container UI health: %w", err)
	}
	owner, err := smokeDocker(ctx, "exec", name, "cat", "/home/graphit/.graphit/daemon/ui.url")
	if err != nil {
		return err
	}
	if owner != "1\nhttp://127.0.0.1:8080/" {
		return fmt.Errorf("wrong daemon owner/bound UI address: %q", owner)
	}
	if pid, err := smokeUIChildPID(ctx, name); err != nil {
		return err
	} else if pid != "" {
		return fmt.Errorf("UI launched an additional process: pid=%q", pid)
	}

	status := http.StatusOK
	if auth {
		status = http.StatusUnauthorized
	}
	if !smokeHTTP(uiAddress+"api/status", status) {
		return fmt.Errorf("container UI auth=%t did not return %d", auth, status)
	}
	if !auth {
		if !smokeHTTP(uiAddress, http.StatusOK) {
			return fmt.Errorf("UI page unavailable")
		}
		smokeMemory(ctx, name, "UI active", out)
		smokeIdleCPU(ctx, name, []string{"1"}, out)
	}

	if _, err := smokeDocker(ctx, "stop", "--time", "10", name); err != nil {
		return err
	}
	fmt.Fprintf(out, "ui-smoke: Docker UI=%t auth=%t health/toggle/stop passed", enabled, auth)
	if enabled {
		fmt.Fprint(out, "; same daemon process/URL owner passed")
	}
	fmt.Fprintln(out)
	return nil
}

// CPU samples describe this isolated, empty-workspace smoke only, not a busy project.
func smokeIdleCPU(ctx context.Context, name string, pids []string, out io.Writer) {
	clock, err := smokeDocker(ctx, "exec", name, "getconf", "CLK_TCK")
	if err != nil {
		return
	}
	hz, err := strconv.ParseFloat(clock, 64)
	if err != nil || hz <= 0 {
		return
	}
	ticks := func(pid string) (int64, bool) {
		data, e := smokeDocker(ctx, "exec", name, "cat", "/proc/"+pid+"/stat")
		if e != nil {
			return 0, false
		}
		end := strings.LastIndex(data, ")")
		if end < 0 {
			return 0, false
		}
		fields := strings.Fields(data[end+1:])
		if len(fields) < 13 {
			return 0, false
		}
		user, e1 := strconv.ParseInt(fields[11], 10, 64)
		system, e2 := strconv.ParseInt(fields[12], 10, 64)
		return user + system, e1 == nil && e2 == nil
	}
	start := time.Now()
	before := make([]int64, len(pids))
	for i, pid := range pids {
		value, ok := ticks(pid)
		if !ok {
			return
		}
		before[i] = value
	}
	timer := time.NewTimer(time.Second)
	defer timer.Stop()
	select {
	case <-ctx.Done():
		return
	case <-timer.C:
	}
	for i, pid := range pids {
		after, ok := ticks(pid)
		if ok {
			fmt.Fprintf(out, "ui-smoke: isolated idle PID %s CPU %.1f%% of one core\n", pid, float64(after-before[i])/hz/time.Since(start).Seconds()*100)
		}
	}
}

func smokeUIChildPID(ctx context.Context, name string) (string, error) {
	// These commands execute inside the Linux image, not the host shell.
	script := `for file in /proc/[0-9]*/cmdline; do cmd=$(tr '\000' ' ' < "$file" 2>/dev/null) || continue; case "$cmd" in /*/graphit-core' ui '*) pid=${file#/proc/}; printf '%s\n' "${pid%/cmdline}"; break;; esac; done`
	value, err := smokeDocker(ctx, "exec", name, "sh", "-c", script)
	if err != nil {
		return "", err
	}
	if value != "" {
		if _, err := strconv.Atoi(value); err != nil {
			return "", fmt.Errorf("invalid child PID")
		}
	}
	return value, nil
}

func smokeDocker(ctx context.Context, args ...string) (string, error) {
	data, err := exec.CommandContext(ctx, "docker", args...).CombinedOutput()
	if err != nil {
		return "", fmt.Errorf("docker %s: %w: %s", args[0], err, strings.TrimSpace(string(data)))
	}
	return strings.TrimSpace(string(data)), nil
}

func smokeHTTP(address string, status int) bool {
	client := http.Client{Timeout: time.Second}
	response, err := client.Get(address)
	if err != nil {
		return false
	}
	defer response.Body.Close()
	return response.StatusCode == status
}

func awaitSmoke(ctx context.Context, ready func() bool) error {
	timeout := time.NewTimer(60 * time.Second)
	defer timeout.Stop()
	ticker := time.NewTicker(100 * time.Millisecond)
	defer ticker.Stop()
	for {
		if ready() {
			return nil
		}
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-timeout.C:
			return fmt.Errorf("timed out")
		case <-ticker.C:
		}
	}
}
