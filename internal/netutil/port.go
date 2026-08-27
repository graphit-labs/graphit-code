package netutil

import (
	"fmt"
	"net"
	"strconv"
)

const maxPortAttempts = 100

func FindFreePort(preferredPort int) (int, error) {
	return FindFreePortOnHost("", preferredPort)
}

func FindFreePortOnHost(host string, preferredPort int) (int, error) {
	ln, port, err := ListenOnFreePortOnHost(host, preferredPort)
	if err != nil {
		return 0, err
	}
	_ = ln.Close()
	return port, nil
}

func ListenOnFreePort(preferredPort int) (net.Listener, int, error) {
	return ListenOnFreePortOnHost("", preferredPort)
}

func ListenOnFreePortOnHost(host string, preferredPort int) (net.Listener, int, error) {
	for i := 0; i < maxPortAttempts; i++ {
		port := preferredPort + i
		ln, err := net.Listen("tcp", net.JoinHostPort(host, strconv.Itoa(port)))
		if err == nil {
			return ln, port, nil
		}
	}
	return nil, 0, fmt.Errorf(
		"no free port found in range %d–%d", preferredPort, preferredPort+maxPortAttempts-1,
	)
}
