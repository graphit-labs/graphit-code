package ast

import (
	"bytes"
	"encoding/binary"
	"encoding/json"
	"fmt"
	"io"
	"os/exec"
	"sync"

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

	// Process pool — buffered channel acts as a semaphore/queue.
	pool     chan *sidecarProcess
	poolSize int
	initOnce sync.Once
	initErr  error
}

// sidecarProcess represents a single long-lived sidecar subprocess.
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
	d.initOnce.Do(func() {
		d.pool = make(chan *sidecarProcess, d.poolSize)
		for i := 0; i < d.poolSize; i++ {
			proc, err := d.startProcess()
			if err != nil {
				d.initErr = fmt.Errorf("sidecar init: %w", err)
				return
			}
			d.pool <- proc
		}
	})
	if d.initErr != nil {
		return nil, d.initErr
	}

	// Acquire a process from the pool.
	proc := <-d.pool

	tree, err := d.callProcess(proc, src)
	if err != nil {
		// Process may be dead — try to restart it.
		proc.close()
		newProc, startErr := d.startProcess()
		if startErr != nil {
			d.pool <- proc // put broken one back to avoid deadlock
			return nil, fmt.Errorf("sidecar restart failed: %w (original: %v)", startErr, err)
		}
		proc = newProc
		// Retry once with fresh process.
		tree, err = d.callProcess(proc, src)
		if err != nil {
			proc.close()
			newProc2, startErr2 := d.startProcess()
			if startErr2 != nil {
				d.pool <- proc
				return nil, fmt.Errorf("sidecar second restart failed: %w", startErr2)
			}
			d.pool <- newProc2
			return nil, fmt.Errorf("sidecar parse failed after restart: %w", err)
		}
	}

	// Return process to pool.
	d.pool <- proc
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
			proc.close()
		default:
			return
		}
	}
}

// startProcess launches a new sidecar subprocess.
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
	// Discard stderr to avoid blocking.
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

// callProcess sends a parse request to the sidecar and reads the response.
func (d *SidecarDriver) callProcess(proc *sidecarProcess, src []byte) (*antlrcommon.TreeNode, error) {
	proc.mu.Lock()
	defer proc.mu.Unlock()

	// Build request frame: [grammar\0][source]
	grammarBytes := append([]byte(d.grammar), 0)
	payload := append(grammarBytes, src...)

	// Write request: [4 bytes length LE][payload]
	length := uint32(len(payload))
	if err := binary.Write(proc.stdin, binary.LittleEndian, length); err != nil {
		return nil, fmt.Errorf("write length: %w", err)
	}
	if _, err := proc.stdin.Write(payload); err != nil {
		return nil, fmt.Errorf("write payload: %w", err)
	}

	// Read response: [4 bytes length LE][1 byte status][JSON payload]
	var respLength uint32
	if err := binary.Read(proc.stdout, binary.LittleEndian, &respLength); err != nil {
		return nil, fmt.Errorf("read response length: %w", err)
	}

	respBuf := make([]byte, respLength)
	if _, err := io.ReadFull(proc.stdout, respBuf); err != nil {
		return nil, fmt.Errorf("read response payload (%d bytes): %w", respLength, err)
	}

	if len(respBuf) < 1 {
		return nil, fmt.Errorf("empty response")
	}

	status := respBuf[0]
	jsonPayload := respBuf[1:]

	if status != 0 {
		return nil, fmt.Errorf("sidecar error: %s", string(jsonPayload))
	}

	// Deserialize JSON to TreeNode.
	var tree antlrcommon.TreeNode
	dec := json.NewDecoder(bytes.NewReader(jsonPayload))
	if err := dec.Decode(&tree); err != nil {
		return nil, fmt.Errorf("json decode: %w", err)
	}
	return &tree, nil
}

// close terminates the sidecar process.
func (p *sidecarProcess) close() {
	if p.stdin != nil {
		p.stdin.Close()
	}
	if p.cmd != nil && p.cmd.Process != nil {
		_ = p.cmd.Process.Kill()
		_ = p.cmd.Wait()
	}
}
