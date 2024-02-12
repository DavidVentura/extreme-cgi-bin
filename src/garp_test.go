package main

import (
	"bytes"
	"encoding/hex"
	"net"
	"testing"
)

var slash24 = net.IPMask{255, 255, 255, 0}

func TestGarpPayload(t *testing.T) {
	ip := net.IP{192, 168, 1, 1}
	mac := net.HardwareAddr{0x06, 0x00, 0xac, 0x10, 0x00, 0x02}

	bEth := net.HardwareAddr{0xff, 0xff, 0xff, 0xff, 0xff, 0xff}

	want := []byte{
		0x00, 0x01,
		0x08, 0x00, // ipv4
		0x06,       // hwAddrLe
		0x04,       // iplen
		0x00, 0x02, //reply
		mac[0], mac[1], mac[2], mac[3], mac[4], mac[5], // sender mac
		ip[0], ip[1], ip[2], ip[3], // sender ip
		bEth[0], bEth[1], bEth[2], bEth[3], bEth[4], bEth[5], // receiver mac (broadcast)
		ip[0], ip[1], ip[2], ip[3], // receiver ip (same as sender)
	}
	got := garpPayload(mac, ip, slash24)
	if !bytes.Equal(got, want) {
		t.Fatalf("bad GarpPayload, \n got: %s\nwant: %s", hex.EncodeToString(got), hex.EncodeToString(want))
	}
}
