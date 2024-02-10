package main

import (
	"encoding/binary"
	"log/slog"
	"net"

	"github.com/mdlayher/ethernet"
	"github.com/songgao/ether"
)

func arpNetInterfaces() []net.Interface {
	ret := []net.Interface{}
	ifas, err := net.Interfaces()
	if err != nil {
		panic(err)
	}
	for _, ifa := range ifas {
		if ifa.Flags&net.FlagLoopback == net.FlagLoopback {
			continue
		}
		if ifa.Flags&net.FlagUp == 0 {
			continue
		}
		slog.Debug("Found netdev %s with hwaddr %s\n", ifa.Name, ifa.HardwareAddr.String())
		ret = append(ret, ifa)
	}
	return ret
}

func sendGarpIface(ifa net.Interface) {
	ipas, _ := ifa.Addrs()
	for _, ipa := range ipas {
		slog.Debug("net: %s, str: %s\n", ipa.Network(), ipa.String())
		ip, net, _ := net.ParseCIDR(ipa.String())
		if ip.To4() != nil {
			slog.Debug("ip: %s, net: %s\n", ip.String(), net)
			garp := makeGarp(ifa.HardwareAddr, ip, net.Mask)
			garpBytes, _ := garp.MarshalBinary()
			dev, err := ether.NewDev(&ifa, nil)
			if err != nil {
				slog.Error("Could not send ARP", "interface", ifa.Name, "error", err)
				continue
			}
			err = dev.Write(garpBytes)
			if err != nil {
				slog.Error("Could not send ARP", "interface", ifa.Name, "error", err)
				continue
			}
			slog.Info("ARP Sent", "interface", ifa.Name, "hwaddr", ifa.HardwareAddr.String(), "ip", ip.String())
		}
	}
}
func broadcastAddr(ip net.IP, mask net.IPMask) net.IP {
	ret := make(net.IP, 4)
	maskIp := net.IP(mask).To4()
	addrU32 := binary.BigEndian.Uint32(ip.To4()) | ^binary.BigEndian.Uint32(maskIp)
	binary.BigEndian.PutUint32(ret, addrU32)
	return ret
}

func garpPayload(mac net.HardwareAddr, ip net.IP, mask net.IPMask) []byte {
	// https://datatracker.ietf.org/doc/html/rfc826
	// https://datatracker.ietf.org/doc/html/rfc5227
	ip = ip.To4()
	b := make([]byte, 28)
	// ethernet
	hwType := 1
	protoType := uint16(ethernet.EtherTypeIPv4)
	hwAddrLen := uint8(6)    // mac len
	protoAddrLen := uint8(4) // ipv4 len
	opcode := uint16(2)      // reply
	broadcastIp := broadcastAddr(ip, mask)

	binary.BigEndian.PutUint16(b[0:2], uint16(hwType))
	binary.BigEndian.PutUint16(b[2:4], protoType)
	b[4] = hwAddrLen
	b[5] = protoAddrLen
	binary.BigEndian.PutUint16(b[6:8], opcode)
	copy(b[8:8+hwAddrLen], mac)
	copy(b[14:14+protoAddrLen], ip)
	copy(b[18:18+hwAddrLen], ethernet.Broadcast)
	copy(b[24:24+protoAddrLen], broadcastIp)
	return b
}

func makeGarp(mac net.HardwareAddr, ip net.IP, mask net.IPMask) ethernet.Frame {
	payload := garpPayload(mac, ip, mask)
	return ethernet.Frame{
		Destination: ethernet.Broadcast,
		Source:      mac,
		EtherType:   ethernet.EtherTypeARP,
		Payload:     payload,
	}
}
