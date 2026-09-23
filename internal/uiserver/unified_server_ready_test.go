package uiserver

import (
	"context"
	"net"
	"net/http"
	"testing"
	"time"
)

func TestStartWithReadyPublishesOnlyAfterBind(t *testing.T) {
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	port := listener.Addr().(*net.TCPAddr).Port
	server := &UnifiedServer{host: "127.0.0.1", port: port, mux: http.NewServeMux()}
	called := false
	if err := server.StartWithReady(context.Background(), func() error { called = true; return nil }); err == nil || called {
		t.Fatalf("occupied address: error=%v ready called=%t", err, called)
	}
	_ = listener.Close()

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	ready := make(chan struct{})
	done := make(chan error, 1)
	go func() {
		done <- server.StartWithReady(ctx, func() error { close(ready); return nil })
	}()
	select {
	case <-ready:
	case <-time.After(2 * time.Second):
		t.Fatal("server never bound its selected address")
	}
	conn, err := net.DialTimeout("tcp", listener.Addr().String(), time.Second)
	if err != nil {
		t.Fatalf("published address was not listening: %v", err)
	}
	_ = conn.Close()
	cancel()
	if err := <-done; err != nil {
		t.Fatal(err)
	}
}
