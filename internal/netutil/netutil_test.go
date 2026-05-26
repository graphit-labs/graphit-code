package netutil

import (
	"net"
	"testing"
)

func TestFindFreePortSuccess(t *testing.T) {
	// Let's find a free port starting near 12000
	port, err := FindFreePort(12000)
	if err != nil {
		t.Fatalf("expected to find free port, got error: %v", err)
	}
	if port < 12000 || port > 12000+maxPortAttempts {
		t.Errorf("expected port in range [12000, %d], got %d", 12000+maxPortAttempts, port)
	}
}

func TestListenOnFreePortSuccess(t *testing.T) {
	ln, port, err := ListenOnFreePort(13000)
	if err != nil {
		t.Fatalf("expected to listen on free port, got error: %v", err)
	}
	defer func() { _ = ln.Close() }()

	if port < 13000 || port > 13000+maxPortAttempts {
		t.Errorf("expected port in range [13000, %d], got %d", 13000+maxPortAttempts, port)
	}

	// Verify we can't double-listen on the same port
	ln2, err := net.Listen("tcp", ln.Addr().String())
	if err == nil {
		_ = ln2.Close()
		t.Errorf("expected second listen on the same address to fail, but it succeeded")
	}
}

func TestFindFreePortFailure(t *testing.T) {
	// Use an invalid port range (> 65535) to force net.Listen to fail for all attempts
	_, err := FindFreePort(999999)
	if err == nil {
		t.Fatal("expected failure when finding free port in invalid range, but got nil error")
	}
}

func TestListenOnFreePortFailure(t *testing.T) {
	// Use an invalid port range (> 65535) to force net.Listen to fail for all attempts
	_, _, err := ListenOnFreePort(999999)
	if err == nil {
		t.Fatal("expected failure when listening on free port in invalid range, but got nil error")
	}
}
