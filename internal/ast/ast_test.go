package ast

import (
	"os"
	"testing"
)

func TestAstIgnoreAndThrottle(t *testing.T) {
	tempDir, err := os.MkdirTemp("", "graphit-ast-test-*")
	if err != nil {
		t.Fatalf("failed to create temp dir: %v", err)
	}
	defer func() { _ = os.RemoveAll(tempDir) }()

	// 1. NewAstIgnoreChecker
	checker := NewAstIgnoreChecker(tempDir)
	if checker == nil {
		t.Fatal("expected non-nil IgnoreChecker")
	}

	// 2. SafeWorkers
	workersZero := SafeWorkers(0)
	if workersZero < 2 {
		t.Errorf("expected SafeWorkers(0) to be >= 2, got %d", workersZero)
	}

	workersExplicit := SafeWorkers(1)
	if workersExplicit != 1 {
		t.Errorf("expected SafeWorkers(1) to be 1, got %d", workersExplicit)
	}

	workersHuge := SafeWorkers(99999)
	if workersHuge >= 99999 {
		t.Errorf("expected SafeWorkers(99999) to be capped, got %d", workersHuge)
	}
}
