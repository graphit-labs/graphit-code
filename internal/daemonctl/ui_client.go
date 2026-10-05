package daemonctl

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net"
	"net/http"
	"net/url"
	"os"
	"strconv"
	"strings"
	"time"
)

// RequestUIURL reuses a running daemon. It never hands a foreground daemon to
// the service manager or starts another UI process.
func RequestUIURL(ctx context.Context) (string, error) {
	ctx, cancel := context.WithTimeout(ctx, 35*time.Second)
	defer cancel()
	if err := ctx.Err(); err != nil {
		return "", err
	}
	if address := PublishedUIURL(); address != "" {
		return address, nil
	}
	tick := time.NewTicker(50 * time.Millisecond)
	defer tick.Stop()
	var port, key []byte
	for {
		if !IsRunning() {
			return "", fmt.Errorf("daemon is not running; start the daemon first")
		}
		var pe, ke error
		port, pe = os.ReadFile(PortFilePath())
		key, ke = os.ReadFile(KeyFilePath())
		if pe == nil && ke == nil {
			break
		}
		if pe != nil && !os.IsNotExist(pe) {
			return "", fmt.Errorf("reading daemon port: %w", pe)
		}
		if ke != nil && !os.IsNotExist(ke) {
			return "", fmt.Errorf("reading daemon key: %w", ke)
		}
		select {
		case <-ctx.Done():
			return "", fmt.Errorf("waiting for daemon control endpoint: %w", ctx.Err())
		case <-tick.C:
		}
	}
	n, err := strconv.Atoi(strings.TrimSpace(string(port)))
	if err != nil || n < 1 || n > 65535 {
		return "", fmt.Errorf("invalid daemon control port")
	}
	token := strings.TrimSpace(string(key))
	if token == "" {
		return "", fmt.Errorf("daemon runtime key is unavailable")
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, "http://"+net.JoinHostPort("127.0.0.1", strconv.Itoa(n))+UIStartPath, nil)
	if err != nil {
		return "", err
	}
	req.Header.Set("Authorization", "Bearer "+token)
	// Control remains local even when the shell defines a proxy or the server redirects.
	transport := http.DefaultTransport.(*http.Transport).Clone()
	transport.Proxy = nil
	defer transport.CloseIdleConnections()
	client := &http.Client{Transport: transport, CheckRedirect: func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse }}
	resp, err := client.Do(req)
	if err != nil {
		return "", fmt.Errorf("requesting daemon UI: %w", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return "", fmt.Errorf("daemon UI activation returned HTTP %d", resp.StatusCode)
	}
	var body struct {
		URL string `json:"url"`
	}
	if err := json.NewDecoder(io.LimitReader(resp.Body, 4096)).Decode(&body); err != nil {
		return "", fmt.Errorf("invalid daemon UI response: %w", err)
	}
	parsed, err := url.Parse(body.URL)
	if err != nil || parsed.Scheme != "http" || parsed.Hostname() == "" || parsed.Port() == "" || parsed.User != nil || parsed.RawQuery != "" || parsed.Fragment != "" || parsed.Path != "/" {
		return "", fmt.Errorf("invalid daemon UI address")
	}
	return parsed.String(), nil
}
