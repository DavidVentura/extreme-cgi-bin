package main

import (
	"context"
	"encoding/json"
	"log/slog"
	"net"
	"net/http"
	"os"
	"time"
)

func httpServer(done chan int, iface net.Interface, bootTime time.Time) {
	server := &http.Server{
		Addr: ":8080",
		ConnState: func(c net.Conn, cs http.ConnState) {
			if cs == http.StateClosed {
				done <- 1
			}
		},
	}

	shutdownCtx, _ := context.WithTimeout(context.Background(), 1*time.Millisecond)
	ips, _ := iface.Addrs()
	http.HandleFunc("/", func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		w.Header().Set("Connection", "close")
		now := time.Now()
		json.NewEncoder(w).Encode(struct {
			Ip                        string
			Mac                       string
			TimeSinceLaunchingPid1_us int64
		}{
			Ip:                        ips[0].String(),
			Mac:                       iface.HardwareAddr.String(),
			TimeSinceLaunchingPid1_us: now.Sub(bootTime).Microseconds(),
		})
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
	boot := time.Now()

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
	ifaces := arpNetInterfaces()
	for _, ifa := range ifaces {
		sendGarpIface(ifa)
	}

	httpServer(done, ifaces[0], boot)

	<-done
	slog.Info("Done")
}
