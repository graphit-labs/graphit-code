package ast

import (
	"bytes"
	"encoding/binary"
	"encoding/json"
	"fmt"
	"io"
	"log/slog"
	"os/exec"
	"sync"
	"time"

	antlrcommon "github.com/graphit-labs/graphit-code/internal/ast/antlr/common"
)

// SidecarDriver implements antlrcommon.GrammarDriver by delegating to an
// external sidecar process. Communication uses a simple length-prefixed
// binary protocol over stdin/stdout.
//
// SidecarDriver manages a pool of long-lived sidecar processes for amortizing
// process startup cost. Processes are lazily started on first use.
type SidecarDriver struct {
	binaryPath string
	grammar    string

	pool     chan *sidecarProcess
	poolSize int
	initOnce sync.Once
}

const maxSidecarFrame = 256 << 20

type sidecarProcess struct {
	cmd    *exec.Cmd
	stdin  io.WriteCloser
	stdout io.Reader
	mu     sync.Mutex
}

// NewSidecarDriver creates a new SidecarDriver that will delegate parsing
// of the given grammar to the sidecar binary at binaryPath.
// poolSize controls how many concurrent sidecar processes to maintain.
func NewSidecarDriver(binaryPath, grammar string, poolSize int) *SidecarDriver {
	if poolSize < 1 {
		poolSize = 1
	}
	return &SidecarDriver{
		binaryPath: binaryPath,
		grammar:    grammar,
		poolSize:   poolSize,
	}
}

// Parse sends src to the sidecar process and returns the deserialized TreeNode.
func (d *SidecarDriver) Parse(src []byte) (*antlrcommon.TreeNode, error) {
	// Slots exist from the start; the processes behind them do not. Spawning
	// eagerly here meant a single failure left the already-started processes
	// running with no way to reach them, and made every later call fail with the
	// stored init error even if the cause had gone away.
	d.initOnce.Do(func() {
		d.pool = make(chan *sidecarProcess, d.poolSize)
		for i := 0; i < d.poolSize; i++ {
			d.pool <- nil
		}
	})

	proc := <-d.pool
	if proc == nil {
		started, err := d.startProcess()
		if err != nil {
			d.pool <- nil
			return nil, fmt.Errorf("sidecar start: %w", err)
		}
		proc = started
	}

	tree, err := d.callProcess(proc, src)
	if err == nil {
		d.pool <- proc
		return tree, nil
	}

	proc.close()
	replacement, startErr := d.startProcess()
	if startErr != nil {
		d.pool <- nil
		return nil, fmt.Errorf("sidecar restart failed: %w (original: %v)", startErr, err)
	}

	tree, retryErr := d.callProcess(replacement, src)
	if retryErr != nil {
		replacement.close()
		d.pool <- nil
		return nil, fmt.Errorf("sidecar parse failed after restart: %w", retryErr)
	}

	d.pool <- replacement
	return tree, nil
}

// Close shuts down all sidecar processes in the pool.
func (d *SidecarDriver) Close() {
	if d.pool == nil {
		return
	}
	for i := 0; i < d.poolSize; i++ {
		select {
		case proc := <-d.pool:
			if proc != nil {
				proc.close()
			}
		case <-time.After(2 * time.Second):
			slog.Warn("sidecar: giving up on a slot still in use at shutdown",
				"grammar", d.grammar, "slot", i)
		}
	}
}

func (d *SidecarDriver) startProcess() (*sidecarProcess, error) {
	cmd := exec.Command(d.binaryPath)
	stdinPipe, err := cmd.StdinPipe()
	if err != nil {
		return nil, fmt.Errorf("stdin pipe: %w", err)
	}
	stdoutPipe, err := cmd.StdoutPipe()
	if err != nil {
		stdinPipe.Close()
		return nil, fmt.Errorf("stdout pipe: %w", err)
	}
	cmd.Stderr = nil

	if err := cmd.Start(); err != nil {
		return nil, fmt.Errorf("start sidecar: %w", err)
	}

	return &sidecarProcess{
		cmd:    cmd,
		stdin:  stdinPipe,
		stdout: stdoutPipe,
	}, nil
}

func (d *SidecarDriver) callProcess(proc *sidecarProcess, src []byte) (*antlrcommon.TreeNode, error) {
	proc.mu.Lock()
	defer proc.mu.Unlock()

	grammarBytes := append([]byte(d.grammar), 0)
	payload := append(grammarBytes, src...)

	length := uint32(len(payload))
	if err := binary.Write(proc.stdin, binary.LittleEndian, length); err != nil {
		return nil, fmt.Errorf("write length: %w", err)
	}
	if _, err := proc.stdin.Write(payload); err != nil {
		return nil, fmt.Errorf("write payload: %w", err)
	}

	var respLength uint32
	if err := binary.Read(proc.stdout, binary.LittleEndian, &respLength); err != nil {
		return nil, fmt.Errorf("read response length: %w", err)
	}

	if respLength > maxSidecarFrame {
		return nil, fmt.Errorf("sidecar response frame too large: %d bytes (limit %d) — "+
			"the stream is out of sync", respLength, maxSidecarFrame)
	}

	var respBody bytes.Buffer
	respBody.Grow(int(min(respLength, 1<<16)))
	if _, err := io.CopyN(&respBody, proc.stdout, int64(respLength)); err != nil {
		return nil, fmt.Errorf("read response payload (%d bytes): %w", respLength, err)
	}
	respBuf := respBody.Bytes()

	if len(respBuf) < 1 {
		return nil, fmt.Errorf("empty response")
	}

	status := respBuf[0]
	jsonPayload := respBuf[1:]

	if status != 0 {
		return nil, fmt.Errorf("sidecar error: %s", string(jsonPayload))
	}

	var tree antlrcommon.TreeNode
	dec := json.NewDecoder(bytes.NewReader(jsonPayload))
	if err := dec.Decode(&tree); err != nil {
		return nil, fmt.Errorf("json decode: %w", err)
	}
	return &tree, nil
}

func (p *sidecarProcess) close() {
	if p.stdin != nil {
		p.stdin.Close()
	}
	if p.cmd != nil && p.cmd.Process != nil {
		_ = p.cmd.Process.Kill()
		_ = p.cmd.Wait()
	}
}
