package main

import (
	"context"
	"fmt"
	"html"
	"log/slog"
	"net"
	"net/http"
	"os"
	"time"
)

func httpServer(done chan int) {
	server := &http.Server{
		Addr: ":8080",
		ConnState: func(c net.Conn, cs http.ConnState) {
			if cs == http.StateClosed {
				done <- 1
			}
		},
	}

	shutdownCtx, _ := context.WithTimeout(context.Background(), 1*time.Millisecond)
	http.HandleFunc("/bar", func(w http.ResponseWriter, r *http.Request) {
		fmt.Fprintf(w, "Hello, %q\n", html.EscapeString(r.URL.Path))
		fmt.Fprintln(w, "bye, time to spawn a new computer")
		if f, ok := w.(http.Flusher); ok {
			f.Flush()
		}
		server.Shutdown(shutdownCtx)
	})

	slog.Info("Server starting now")
	listener, err := net.Listen("tcp", ":8081")
	if err != nil {
		slog.Error("big sad")
		os.Exit(1)
	}
	server.Serve(listener)
}
func main() {
	logger := slog.New(slog.NewTextHandler(os.Stdout, nil))
	slog.SetDefault(logger)

	slog.Info("Process started")
	done := make(chan int, 1)

	// ARP is requested every 1 second
	// if we happen to make an ARP-request before this VM is live
	// we will have to wait 1 second to send a followup request
	//
	// we can bypass that by sending a gratuitous ARP reply
	// on boot, to populate the ARP table of the host
	for _, ifa := range arpNetInterfaces() {
		sendGarpIface(ifa)
	}

	httpServer(done)

	<-done
	slog.Info("Done")
}
