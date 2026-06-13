// graphit-grammar-pack creates .grammar fat archive files from platform-specific
// shared libraries. It packages one or more platform-specific shared libraries
// into a single distributable file using zstd compression.
//
// Usage:
//
//	graphit-grammar-pack -o output.grammar \
//	    -platform linux/amd64:path/to/lib.so \
//	    -platform darwin/arm64:path/to/lib.dylib \
//	    -symbol tree_sitter_go
package main

import (
	"flag"
	"fmt"
	"os"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/ast"
)

type platformFlags []string

func (pf *platformFlags) String() string { return strings.Join(*pf, ", ") }
func (pf *platformFlags) Set(value string) error {
	*pf = append(*pf, value)
	return nil
}

func main() {
	var (
		output    string
		symbol    string
		platforms platformFlags
	)

	flag.StringVar(&output, "o", "", "output .grammar file path (required)")
	flag.StringVar(&symbol, "symbol", "", "symbol name exported by the grammar, e.g. tree_sitter_go (required)")
	flag.Var(&platforms, "platform", "platform spec: OS/ARCH:path (repeatable, e.g. linux/amd64:lib.so)")
	flag.Parse()

	if output == "" || symbol == "" || len(platforms) == 0 {
		fmt.Fprintf(os.Stderr, "Usage: %s -o output.grammar -symbol tree_sitter_go -platform linux/amd64:lib.so [-platform ...]\n", os.Args[0])
		flag.PrintDefaults()
		os.Exit(1)
	}

	var grammarPlatforms []ast.GrammarPlatform

	for _, spec := range platforms {
		// Parse "OS/ARCH:path"
		colonIdx := strings.Index(spec, ":")
		if colonIdx < 0 {
			fmt.Fprintf(os.Stderr, "error: invalid platform spec %q (expected OS/ARCH:path)\n", spec)
			os.Exit(1)
		}

		osArch := spec[:colonIdx]
		filePath := spec[colonIdx+1:]

		parts := strings.SplitN(osArch, "/", 2)
		if len(parts) != 2 || parts[0] == "" || parts[1] == "" {
			fmt.Fprintf(os.Stderr, "error: invalid OS/ARCH %q (expected e.g. linux/amd64)\n", osArch)
			os.Exit(1)
		}

		data, err := os.ReadFile(filePath)
		if err != nil {
			fmt.Fprintf(os.Stderr, "error: reading %q: %v\n", filePath, err)
			os.Exit(1)
		}

		fmt.Printf("  platform %s/%s: %s (%d bytes)\n", parts[0], parts[1], filePath, len(data))

		grammarPlatforms = append(grammarPlatforms, ast.GrammarPlatform{
			OS:         parts[0],
			Arch:       parts[1],
			SymbolName: symbol,
			Data:       data,
		})
	}

	fmt.Printf("Writing %s with %d platform(s), symbol=%s\n", output, len(grammarPlatforms), symbol)

	if err := ast.WriteGrammarArchive(output, grammarPlatforms); err != nil {
		fmt.Fprintf(os.Stderr, "error: %v\n", err)
		os.Exit(1)
	}

	// Verify by reading back.
	archive, err := ast.ReadGrammarArchive(output)
	if err != nil {
		fmt.Fprintf(os.Stderr, "error: verify read: %v\n", err)
		os.Exit(1)
	}

	info, _ := os.Stat(output)
	fmt.Printf("Created %s (%d bytes, %d platform(s))\n", output, info.Size(), len(archive.Platforms))
	for _, p := range archive.Platforms {
		fmt.Printf("  %s/%s symbol=%s\n", p.OS, p.Arch, p.SymbolName)
	}
}
