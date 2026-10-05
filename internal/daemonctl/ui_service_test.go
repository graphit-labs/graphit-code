package daemonctl

import (
	"context"
	"errors"
	"sync"
	"sync/atomic"
	"testing"
	"time"
)

func TestUIServiceWaitsForRequest(t *testing.T) {
	var calls atomic.Int32
	s := NewUIService(func(ctx context.Context) error { calls.Add(1); <-ctx.Done(); return ctx.Err() })
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan error, 1)
	go func() { done <- s.Start(ctx) }()
	select {
	case <-done:
		t.Fatal("idle module exited")
	case <-time.After(20 * time.Millisecond):
	}
	cancel()
	if !errors.Is(<-done, context.Canceled) || calls.Load() != 0 {
		t.Fatal("idle UI constructed a server")
	}
}

func TestUIServiceConcurrentRequestsWaitForReadiness(t *testing.T) {
	var calls atomic.Int32
	var ready atomic.Bool
	s := NewUIService(func(ctx context.Context) error { calls.Add(1); <-ctx.Done(); return ctx.Err() })
	s.url = func() string {
		if ready.Load() {
			return "http://127.0.0.1:8080/"
		}
		return ""
	}
	ctx, cancel := context.WithTimeout(context.Background(), time.Second)
	defer cancel()
	done := make(chan error, 1)
	go func() { done <- s.Start(ctx) }()
	var wg sync.WaitGroup
	answers := make(chan error, 20)
	for range 20 {
		wg.Add(1)
		go func() {
			defer wg.Done()
			address, err := s.Ensure(ctx)
			if err == nil && address == "" {
				err = errors.New("empty URL")
			}
			answers <- err
		}()
	}
	select {
	case <-answers:
		t.Fatal("request answered before readiness")
	case <-time.After(30 * time.Millisecond):
	}
	ready.Store(true)
	wg.Wait()
	close(answers)
	for err := range answers {
		if err != nil {
			t.Fatal(err)
		}
	}
	if calls.Load() != 1 {
		t.Fatalf("constructed %d UI instances", calls.Load())
	}
	cancel()
	<-done
}

func TestUIServiceCallerCancellationDoesNotStopServer(t *testing.T) {
	started := make(chan struct{})
	s := NewUIService(func(ctx context.Context) error { close(started); <-ctx.Done(); return ctx.Err() })
	s.url = func() string { return "" }
	lifetime, stop := context.WithCancel(context.Background())
	defer stop()
	done := make(chan error, 1)
	go func() { done <- s.Start(lifetime) }()
	request, cancel := context.WithTimeout(context.Background(), 30*time.Millisecond)
	defer cancel()
	if _, err := s.Ensure(request); !errors.Is(err, context.DeadlineExceeded) {
		t.Fatalf("error=%v", err)
	}
	<-started
	select {
	case <-done:
		t.Fatal("request cancellation stopped server")
	default:
	}
	stop()
	<-done
}

func TestUIServiceRestartRetainsActivation(t *testing.T) {
	var calls int
	want := errors.New("bind failed")
	s := NewUIService(func(context.Context) error { calls++; return want })
	s.Activate()
	for range 2 {
		if !errors.Is(s.Start(context.Background()), want) {
			t.Fatal("lost failure")
		}
	}
	if calls != 2 {
		t.Fatal("restart waited for a second request")
	}
}
