package daemon

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"time"
)

var ErrAlreadyRunning = errors.New("daemon: another instance is already running")

const pidFileName = "daemon.pid"

type PIDFile struct {
	path   string
	lockFD *os.File
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

func (pf *PIDFile) Acquire() error {
	if err := os.MkdirAll(filepath.Dir(pf.path), 0o755); err != nil {
		return fmt.Errorf("creating pid dir: %w", err)
	}

	f, err := os.OpenFile(pf.path, os.O_CREATE|os.O_RDWR, 0o600)
	if err != nil {
		return fmt.Errorf("opening pid file: %w", err)
	}

	if err := flockExclusive(f); err != nil {
		_ = f.Close()
		if flockContended(err) {
			return ErrAlreadyRunning
		}
		return fmt.Errorf("locking pid file: %w", err)
	}

	if err := f.Truncate(0); err != nil {
		flockRelease(f)
		_ = f.Close()
		return fmt.Errorf("truncating pid file: %w", err)
	}
	if _, err := f.Seek(0, 0); err != nil {
		flockRelease(f)
		_ = f.Close()
		return fmt.Errorf("seeking pid file: %w", err)
	}
	content := fmt.Sprintf("%d\n%s\n", os.Getpid(), time.Now().UTC().Format(time.RFC3339))
	if _, err := f.WriteString(content); err != nil {
		flockRelease(f)
		_ = f.Close()
		return fmt.Errorf("writing pid file: %w", err)
	}
	_ = f.Sync()
	if err := writePIDMetadata(pf.path, content); err != nil {
		flockRelease(f)
		_ = f.Close()
		return fmt.Errorf("writing pid metadata: %w", err)
	}

	pf.lockFD = f
	return nil
}

func (pf *PIDFile) Release() {
	if pf.lockFD != nil {
		clearPIDMetadata(pf.path)
		_ = pf.lockFD.Truncate(0)
		flockRelease(pf.lockFD)
		_ = pf.lockFD.Close()
		pf.lockFD = nil
	}
}

func (pf *PIDFile) Write() error {
	if err := os.MkdirAll(filepath.Dir(pf.path), 0o755); err != nil {
		return fmt.Errorf("creating pid dir: %w", err)
	}
	content := fmt.Sprintf("%d\n%s\n", os.Getpid(), time.Now().UTC().Format(time.RFC3339))
	f, err := os.OpenFile(pf.path, os.O_CREATE|os.O_WRONLY, 0o600)
	if err != nil {
		return err
	}
	defer f.Close()
	if err := f.Truncate(0); err != nil {
		return err
	}
	if _, err := f.Seek(0, 0); err != nil {
		return err
	}
	_, err = f.WriteString(content)
	if err != nil {
		return err
	}
	return writePIDMetadata(pf.path, content)
}

func (pf *PIDFile) Remove() {
	f, err := os.OpenFile(pf.path, os.O_RDWR, 0o600)
	if err != nil {
		return
	}
	defer f.Close()
	if err := flockExclusive(f); err != nil {
		return
	}
	defer flockRelease(f)
	// Keep the lock inode stable. Unlinking it while another process has the old
	// file open can split daemon exclusion between old and newly created files;
	// Windows can also reject deletion of an open locked file.
	_ = f.Truncate(0)
	clearPIDMetadata(pf.path)
}

func (pf *PIDFile) Read() (*pidData, error) {
	data, err := os.ReadFile(pf.path)
	if err != nil {
		if alt, altErr := readPIDMetadata(pf.path); altErr == nil {
			data, err = alt, nil
		}
	}
	if os.IsNotExist(err) {
		return nil, nil
	}
	if err != nil {
		return nil, err
	}
	lines := strings.Split(strings.TrimSpace(string(data)), "\n")
	if len(lines) < 1 || strings.TrimSpace(lines[0]) == "" {
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
	f, err := os.OpenFile(pf.path, os.O_RDWR, 0o600)
	if err != nil {
		return nil
	}
	defer f.Close()
	if err := flockExclusive(f); err == nil {
		// A free lock wins over any PID text (which can outlive a crash or be
		// reused by an unrelated process). Clear it without unlinking the inode.
		_ = f.Truncate(0)
		clearPIDMetadata(pf.path)
		flockRelease(f)
		return nil
	} else if !flockContended(err) {
		return nil
	}
	pd, err := pf.Read()
	if err != nil {
		return nil
	}
	if pd == nil {
		return nil
	}
	if !pidIsAlive(pd.PID) {
		return nil
	}
	return pd
}

func (pf *PIDFile) Signal(sig os.Signal) error {
	pd := pf.IsAlive()
	if pd == nil {
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
