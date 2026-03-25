package main

import (
	"log"
	"os"
	"path/filepath"
	"runtime/debug"

	"github.com/bgunnarsson/binman/internal/app"
	"github.com/bgunnarsson/binman/internal/config"
)

func main() {
	var a *app.App

	defer func() {
		if r := recover(); r != nil {
			if a != nil {
				a.TV.Stop() // restore terminal before exit
			}
			log.Printf("PANIC: %v\n%s", r, debug.Stack())
			os.Exit(1)
		}
	}()

	cfg := config.Load()
	if cfg.Collection == "" {
		log.Fatal("HTTP_FILES not set — add 'HTTP_FILES = /path/to/files' to ~/.config/binman/config")
	}
	root := filepath.Clean(cfg.Collection)

	var err error
	a, err = app.New(root)
	if err != nil {
		log.Fatal(err)
	}

	if err := a.Run(); err != nil {
		log.Fatal(err)
	}
}
