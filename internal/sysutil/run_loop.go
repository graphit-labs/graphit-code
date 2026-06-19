package sysutil

import (
	"errors"
	"os"
)

const relayWorkerEnv = "GRAPHIT_RELAY_WORKER"

var ErrReplace = errors.New("sysutil: replace process")

func EffectivePID() int {
	if os.Getenv(relayWorkerEnv) != "" {
		return os.Getppid()
	}
	return os.Getpid()
}
