package commands

import (
	"os"
	"testing"

	_ "github.com/graphit-labs/graphit-code/internal/testsupport"
	"github.com/graphit-labs/graphit-code/internal/testsupport/testenv"
)

func TestMain(m *testing.M) {
	os.Exit(testenv.Run(m))
}
