package netutil

import (
	"net"
	"testing"
)

func TestFindFreePortSuccess(t *testing.T) {
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

	ln2, err := net.Listen("tcp", ln.Addr().String())
	if err == nil {
		_ = ln2.Close()
		t.Errorf("expected second listen on the same address to fail, but it succeeded")
	}
}

func TestListenOnFreePortOnHostUsesTheConfiguredHost(t *testing.T) {
	ln, _, err := ListenOnFreePortOnHost("127.0.0.1", 14000)
	if err != nil {
		t.Fatalf("ListenOnFreePortOnHost: %v", err)
	}
	defer func() { _ = ln.Close() }()

	host, _, err := net.SplitHostPort(ln.Addr().String())
	if err != nil {
		t.Fatalf("SplitHostPort: %v", err)
	}
	if host != "127.0.0.1" {
		t.Fatalf("listener host = %q; want 127.0.0.1", host)
	}
}

func TestFindFreePortFailure(t *testing.T) {
	_, err := FindFreePort(999999)
	if err == nil {
		t.Fatal("expected failure when finding free port in invalid range, but got nil error")
	}
}

func TestListenOnFreePortFailure(t *testing.T) {
	_, _, err := ListenOnFreePort(999999)
	if err == nil {
		t.Fatal("expected failure when listening on free port in invalid range, but got nil error")
	}
}
