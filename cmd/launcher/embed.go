package main

import "embed"

//go:embed runtime/*
var embeddedRuntime embed.FS
