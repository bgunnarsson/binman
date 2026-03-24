package main

import (
	"flag"
	"log"
	"os"
	"path/filepath"
	"runtime/debug"

	"github.com/bgunnarsson/binreq/internal/app"
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

	collection := flag.String("collection", "/Users/bgunnarsson/Development/bgunnarsson/bg-http", "path to .http collections root")
	flag.Parse()

	root := filepath.Clean(*collection)
	if v := os.Getenv("BINREQ_ROOT"); v != "" {
		root = filepath.Clean(v)
	}

	var err error
	a, err = app.New(root)
	if err != nil {
		log.Fatal(err)
	}

	if err := a.Run(); err != nil {
		log.Fatal(err)
	}
}
