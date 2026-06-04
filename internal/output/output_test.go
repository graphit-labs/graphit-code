package output

import (
	"bytes"
	"errors"
	"io"
	"os"
	"os/exec"
	"strings"
	"testing"
	"time"
)

func TestPrinterBasic(t *testing.T) {
	mutedOrig := muted
	defer func() { muted = mutedOrig }()
	muted = false

	// New printer with prefix
	p := NewPrinter("TEST")
	var buf bytes.Buffer
	p = p.WithWriter(&buf)

	if p.prefix != "TEST" {
		t.Errorf("expected prefix 'TEST', got %q", p.prefix)
	}

	// Test Info
	buf.Reset()
	p.Info("hello %s", "world")
	out := buf.String()
	if !strings.Contains(out, "[TEST]") || !strings.Contains(out, "hello world") {
		t.Errorf("Info output unexpected: %q", out)
	}

	// Test Success
	buf.Reset()
	p.Success("great %s", "success")
	out = buf.String()
	if !strings.Contains(out, SymbolOK) || !strings.Contains(out, "great success") {
		t.Errorf("Success output unexpected: %q", out)
	}

	// Test Error
	buf.Reset()
	p.Error("fatal %s", "error")
	out = buf.String()
	if !strings.Contains(out, SymbolError) || !strings.Contains(out, "fatal error") {
		t.Errorf("Error output unexpected: %q", out)
	}

	// Test Warn
	buf.Reset()
	p.Warn("warning %s", "msg")
	out = buf.String()
	if !strings.Contains(out, SymbolWarn) || !strings.Contains(out, "warning msg") {
		t.Errorf("Warn output unexpected: %q", out)
	}

	// Test Running
	buf.Reset()
	p.Running("running %s", "now")
	out = buf.String()
	if !strings.Contains(out, SymbolRunning) || !strings.Contains(out, "running now") {
		t.Errorf("Running output unexpected: %q", out)
	}

	// Test Step
	buf.Reset()
	p.Step("step %s", "one")
	out = buf.String()
	if !strings.Contains(out, SymbolStep) || !strings.Contains(out, "step one") {
		t.Errorf("Step output unexpected: %q", out)
	}

	// Test StepOK
	buf.Reset()
	p.StepOK("step %s", "ok")
	out = buf.String()
	if !strings.Contains(out, SymbolStep) || !strings.Contains(out, SymbolOK) || !strings.Contains(out, "step ok") {
		t.Errorf("StepOK output unexpected: %q", out)
	}

	// Test StepError
	buf.Reset()
	p.StepError("step %s", "err")
	out = buf.String()
	if !strings.Contains(out, SymbolStep) || !strings.Contains(out, SymbolError) || !strings.Contains(out, "step err") {
		t.Errorf("StepError output unexpected: %q", out)
	}

	// Test StepWarn
	buf.Reset()
	p.StepWarn("step %s", "warn")
	out = buf.String()
	if !strings.Contains(out, SymbolStep) || !strings.Contains(out, SymbolWarn) || !strings.Contains(out, "step warn") {
		t.Errorf("StepWarn output unexpected: %q", out)
	}

	// Test Detail
	buf.Reset()
	p.Detail("key", "val")
	out = buf.String()
	if !strings.Contains(out, SymbolStep) || !strings.Contains(out, "key:") || !strings.Contains(out, "val") {
		t.Errorf("Detail output unexpected: %q", out)
	}

	// Test KeyValue
	buf.Reset()
	p.KeyValue("k", "v")
	out = buf.String()
	if !strings.Contains(out, "k:") || !strings.Contains(out, "v") {
		t.Errorf("KeyValue output unexpected: %q", out)
	}

	// Test Data
	buf.Reset()
	p.Data("some raw data")
	out = buf.String()
	if out != "some raw data\n" {
		t.Errorf("Data output unexpected: %q", out)
	}

	// Test Count
	buf.Reset()
	p.Count("item", 1)
	out = buf.String()
	if !strings.Contains(out, "1 item") || strings.Contains(out, "items") {
		t.Errorf("Count singular output unexpected: %q", out)
	}

	buf.Reset()
	p.Count("item", 5)
	out = buf.String()
	if !strings.Contains(out, "5 items") {
		t.Errorf("Count plural output unexpected: %q", out)
	}

	// Test ListItem
	buf.Reset()
	p.ListItem("bullet %d", 3)
	out = buf.String()
	if !strings.Contains(out, SymbolBullet) || !strings.Contains(out, "bullet 3") {
		t.Errorf("ListItem output unexpected: %q", out)
	}

	// Test Header
	buf.Reset()
	p.Header("my header")
	out = buf.String()
	if !strings.Contains(out, "my header") {
		t.Errorf("Header output unexpected: %q", out)
	}

	// Test Divider
	buf.Reset()
	p.Divider()
	out = buf.String()
	if !strings.Contains(out, SymbolDivider) {
		t.Errorf("Divider output unexpected: %q", out)
	}

	// Test Blank
	buf.Reset()
	p.Blank()
	out = buf.String()
	if out != "\n" {
		t.Errorf("Blank output unexpected: %q", out)
	}
}

func TestPrinterNoPrefix(t *testing.T) {
	p := NewPrinter("")
	var buf bytes.Buffer
	p = p.WithWriter(&buf)

	p.Info("hello")
	out := buf.String()
	if strings.Contains(out, "[") {
		t.Errorf("expected no bracket prefix, got %q", out)
	}

	if p.tag() != "" {
		t.Error("expected empty tag for empty prefix")
	}
}

func TestMutedPrinter(t *testing.T) {
	mutedOrig := muted
	defer func() { muted = mutedOrig }()
	Mute()

	p := NewPrinter("TEST")
	if p.w != io.Discard {
		t.Error("expected muted printer to output to io.Discard")
	}
}

func TestTable(t *testing.T) {
	p := NewPrinter("")
	var buf bytes.Buffer
	p = p.WithWriter(&buf)

	headers := [2]string{"col1", "col2"}
	rows := [][2]string{
		{"val1", "val2"},
		{"longer_val", "another"},
	}
	p.Table(headers, rows)
	out := buf.String()
	if !strings.Contains(out, "col1") || !strings.Contains(out, "longer_val") || !strings.Contains(out, "another") {
		t.Errorf("Table output unexpected: %q", out)
	}
}

func TestTTYIsTTY(t *testing.T) {
	// Simple sanity test for IsTTY
	_ = IsTTY()
}

func TestTaskNonTTY(t *testing.T) {
	isTTYOrig := isTTY
	defer func() { isTTY = isTTYOrig }()
	isTTY = false

	p := NewPrinter("TASK")
	var buf bytes.Buffer
	p = p.WithWriter(&buf)

	task := p.StartTask("my task %s", "1")
	out := buf.String()
	if !strings.Contains(out, SymbolRunning) || !strings.Contains(out, "my task 1") {
		t.Errorf("expected initial running print for non-TTY, got %q", out)
	}

	buf.Reset()
	task.Update("updated %s", "2")
	task.Done("completed %s", "3")
	out = buf.String()
	if !strings.Contains(out, SymbolOK) || !strings.Contains(out, "completed 3") {
		t.Errorf("expected done line, got %q", out)
	}
}

func TestTaskTTY(t *testing.T) {
	isTTYOrig := isTTY
	defer func() { isTTY = isTTYOrig }()
	isTTY = true

	p := NewPrinter("TASK")
	var buf bytes.Buffer
	p = p.WithWriter(&buf)

	task := p.StartTask("my task %s", "1")
	// Let spinner run a bit
	time.Sleep(120 * time.Millisecond)

	task.Update("updated %s", "2")
	time.Sleep(100 * time.Millisecond)

	task.Done("completed %s", "3")
	out := buf.String()

	// Should contain clear escape sequence \033[K and done symbol
	if !strings.Contains(out, SymbolOK) || !strings.Contains(out, "completed 3") {
		t.Errorf("expected done TTY output, got %q", out)
	}

	// Try failing another task
	buf.Reset()
	task2 := p.StartTask("fail task")
	time.Sleep(100 * time.Millisecond)
	task2.Fail("failed task msg")
	out = buf.String()
	if !strings.Contains(out, SymbolError) || !strings.Contains(out, "failed task msg") {
		t.Errorf("expected fail TTY output, got %q", out)
	}

	// Make sure calling Done or Fail twice is a no-op
	task2.Done("double done")
	task2.Fail("double fail")
}

// TestFatalAndInterrupted runs the test process as a subprocess to verify exits and output
func TestFatalAndInterrupted(t *testing.T) {
	if os.Getenv("BE_CRASHER") == "fatal" {
		Fatal("crash %s", "now")
		return
	}
	if os.Getenv("BE_CRASHER") == "interrupted" {
		Interrupted()
		return
	}
	if os.Getenv("BE_CRASHER") == "interrupted_muted" {
		Mute()
		Interrupted()
		return
	}

	// Run Fatal subcommand
	cmd := exec.Command(os.Args[0], "-test.run=TestFatalAndInterrupted")
	cmd.Env = append(os.Environ(), "BE_CRASHER=fatal")
	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr
	err := cmd.Run()
	var e *exec.ExitError
	if errors.As(err, &e) && !e.Success() {
		// Expected exit code 1
		errOut := stderr.String()
		if !strings.Contains(errOut, SymbolError) || !strings.Contains(errOut, "crash now") {
			t.Errorf("Fatal output unexpected on stderr: %q", errOut)
		}
	} else {
		t.Errorf("expected Fatal exit code 1, got error: %v", err)
	}

	// Run Interrupted subcommand
	cmd2 := exec.Command(os.Args[0], "-test.run=TestFatalAndInterrupted")
	cmd2.Env = append(os.Environ(), "BE_CRASHER=interrupted")
	var stdout2, stderr2 bytes.Buffer
	cmd2.Stdout = &stdout2
	cmd2.Stderr = &stderr2
	err2 := cmd2.Run()
	var e2 *exec.ExitError
	if errors.As(err2, &e2) {
		exitCode := e2.ExitCode()
		if exitCode != 130 {
			t.Errorf("expected Interrupted exit code 130, got %d", exitCode)
		}
		out := stdout2.String()
		if !strings.Contains(out, SymbolWarn) || !strings.Contains(out, "Interrupted") {
			t.Errorf("Interrupted output unexpected: %q", out)
		}
	} else {
		t.Errorf("expected Interrupted exit code 130, got error: %v", err2)
	}

	// Run muted Interrupted subcommand
	cmd3 := exec.Command(os.Args[0], "-test.run=TestFatalAndInterrupted")
	cmd3.Env = append(os.Environ(), "BE_CRASHER=interrupted_muted")
	var stdout3, stderr3 bytes.Buffer
	cmd3.Stdout = &stdout3
	cmd3.Stderr = &stderr3
	err3 := cmd3.Run()
	var e3 *exec.ExitError
	if errors.As(err3, &e3) {
		exitCode := e3.ExitCode()
		if exitCode != 130 {
			t.Errorf("expected muted Interrupted exit code 130, got %d", exitCode)
		}
		// When muted, there should be no output on stdout
		out := stdout3.String()
		if strings.Contains(out, "Interrupted") {
			t.Errorf("expected no output when muted, got %q", out)
		}
	} else {
		t.Errorf("expected muted Interrupted exit, got error: %v", err3)
	}
}

func TestTaskTTYDoneWhileSpinnerRunning(t *testing.T) {
	// This tests the branch in the spinner goroutine where it checks t.done
	// inside the ticker.C case and returns early (lines 232-234).
	// We set t.done = true directly (without closing stopCh) so the goroutine's
	// select can only pick ticker.C and will observe done==true.
	isTTYOrig := isTTY
	defer func() { isTTY = isTTYOrig }()
	isTTY = true

	p := NewPrinter("TASK")
	var buf bytes.Buffer
	p = p.WithWriter(&buf)

	task := p.StartTask("quick task")

	// Wait for at least one spinner frame to render (proves goroutine is running)
	time.Sleep(120 * time.Millisecond)

	// Set done=true WITHOUT closing stopCh, so the goroutine's select
	// can only receive from ticker.C and will hit the if-t.done branch
	task.mu.Lock()
	task.done = true
	task.mu.Unlock()

	// Wait for the goroutine to pick up the ticker.C case and exit
	time.Sleep(200 * time.Millisecond)

	// Clean up by closing stopCh (safe since done is already true and goroutine exited)
	close(task.stopCh)
}


func TestTaskNonTTYFail(t *testing.T) {
	isTTYOrig := isTTY
	defer func() { isTTY = isTTYOrig }()
	isTTY = false

	p := NewPrinter("TASK")
	var buf bytes.Buffer
	p = p.WithWriter(&buf)

	task := p.StartTask("fail task")
	buf.Reset()
	task.Fail("failed %s", "msg")
	out := buf.String()
	if !strings.Contains(out, SymbolError) || !strings.Contains(out, "failed msg") {
		t.Errorf("expected fail output in non-TTY, got %q", out)
	}
}

func TestTaskDoubleFailNonTTY(t *testing.T) {
	isTTYOrig := isTTY
	defer func() { isTTY = isTTYOrig }()
	isTTY = false

	p := NewPrinter("TASK")
	var buf bytes.Buffer
	p = p.WithWriter(&buf)

	task := p.StartTask("task")
	task.Fail("first fail")
	buf.Reset()
	task.Fail("second fail")
	task.Done("second done")
	out := buf.String()
	if out != "" {
		t.Errorf("expected no output after double fail/done, got %q", out)
	}
}

