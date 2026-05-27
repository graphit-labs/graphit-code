package daemon

import (
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"syscall"
	"time"
)

const pidFileName = "daemon.pid"

type PIDFile struct {
	path string
}

func NewPIDFile() *PIDFile {
	return &PIDFile{
		path: filepath.Join(GlobalDaemonDir(), pidFileName),
	}
}

type pidData struct {
	PID       int
	StartedAt time.Time
}

func (pf *PIDFile) Write() error {
	if err := os.MkdirAll(filepath.Dir(pf.path), 0o755); err != nil {
		return fmt.Errorf("creating pid dir: %w", err)
	}
	content := fmt.Sprintf("%d\n%s\n", os.Getpid(), time.Now().UTC().Format(time.RFC3339))
	return os.WriteFile(pf.path, []byte(content), 0o644)
}

func (pf *PIDFile) Remove() {
	_ = os.Remove(pf.path)
}

func (pf *PIDFile) Read() (*pidData, error) {
	data, err := os.ReadFile(pf.path)
	if os.IsNotExist(err) {
		return nil, nil
	}
	if err != nil {
		return nil, err
	}
	lines := strings.Split(strings.TrimSpace(string(data)), "\n")
	if len(lines) < 1 {
		return nil, fmt.Errorf("malformed pid file")
	}
	pid, err := strconv.Atoi(strings.TrimSpace(lines[0]))
	if err != nil {
		return nil, fmt.Errorf("invalid pid: %w", err)
	}
	pd := &pidData{PID: pid}
	if len(lines) >= 2 {
		pd.StartedAt, _ = time.Parse(time.RFC3339, strings.TrimSpace(lines[1]))
	}
	return pd, nil
}

func (pf *PIDFile) IsAlive() *pidData {
	pd, err := pf.Read()
	if err != nil || pd == nil {
		return nil
	}

	proc, err := os.FindProcess(pd.PID)
	if err != nil {
		return nil
	}
	if err := proc.Signal(syscall.Signal(0)); err != nil {

		pf.Remove()
		return nil
	}
	return pd
}

func (pf *PIDFile) Signal(sig syscall.Signal) error {
	pd, err := pf.Read()
	if err != nil || pd == nil {
		return fmt.Errorf("no daemon running (no pid file)")
	}
	proc, err := os.FindProcess(pd.PID)
	if err != nil {
		return fmt.Errorf("process %d not found: %w", pd.PID, err)
	}
	return proc.Signal(sig)
}

func (pf *PIDFile) SignalOS(sig os.Signal) error {
	pd, err := pf.Read()
	if err != nil || pd == nil {
		return fmt.Errorf("no daemon running (no pid file)")
	}
	proc, err := os.FindProcess(pd.PID)
	if err != nil {
		return fmt.Errorf("process %d not found: %w", pd.PID, err)
	}
	return proc.Signal(sig)
}

func (pf *PIDFile) Path() string {
	return pf.path
}
