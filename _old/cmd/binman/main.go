package main

import (
	"crypto/tls"
	"log"
	"os"
	"path/filepath"
	"runtime/debug"
	"time"

	"github.com/bgunnarsson/binman/internal/app"
	"github.com/bgunnarsson/binman/internal/config"
	"github.com/bgunnarsson/binman/internal/httpclient"
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

	timeout := cfg.Timeout
	if timeout == 0 {
		timeout = 30 * time.Second
	}
	var tlsConfig *tls.Config
	if cfg.ClientCert != "" && cfg.ClientKey != "" {
		cert, err := tls.LoadX509KeyPair(cfg.ClientCert, cfg.ClientKey)
		if err != nil {
			log.Fatalf("loading client cert/key: %v", err)
		}
		tlsConfig = &tls.Config{Certificates: []tls.Certificate{cert}}
	}
	httpclient.Configure(timeout, tlsConfig)

	var err error
	a, err = app.New(root)
	if err != nil {
		log.Fatal(err)
	}

	if err := a.Run(); err != nil {
		log.Fatal(err)
	}
}
