package netutil

import (
	"fmt"
	"net"
)

const maxPortAttempts = 100

func FindFreePort(preferredPort int) (int, error) {
	ln, port, err := ListenOnFreePort(preferredPort)
	if err != nil {
		return 0, err
	}
	ln.Close()
	return port, nil
}

func ListenOnFreePort(preferredPort int) (net.Listener, int, error) {
	for i := 0; i < maxPortAttempts; i++ {
		port := preferredPort + i
		ln, err := net.Listen("tcp", fmt.Sprintf(":%d", port))
		if err == nil {
			return ln, port, nil
		}
	}
	return nil, 0, fmt.Errorf(
		"no free port found in range %d–%d", preferredPort, preferredPort+maxPortAttempts-1,
	)
}
