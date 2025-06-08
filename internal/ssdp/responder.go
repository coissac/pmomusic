package ssdp

import (
	"fmt"
	"log"
	"net"
	"strings"
	"time"
)

const (
	ssdpAddr    = "239.255.255.250:1900"
	serviceType = "urn:schemas-upnp-org:device:MediaRenderer:1"
)

// StartSSDPResponder listens for M-SEARCH requests and responds if they match
func StartSSDPResponder(usn, location string) {
	addr, err := net.ResolveUDPAddr("udp4", ssdpAddr)
	if err != nil {
		log.Fatal("Failed to resolve SSDP address:", err)
	}

	conn, err := net.ListenMulticastUDP("udp4", nil, addr)
	if err != nil {
		log.Fatal("Failed to listen for SSDP:", err)
	}
	defer conn.Close()

	conn.SetReadBuffer(2048)

	buf := make([]byte, 2048)
	log.Println("🔍 Listening for M-SEARCH...")

	for {
		n, src, err := conn.ReadFromUDP(buf)
		if err != nil {
			log.Println("ReadFromUDP error:", err)
			continue
		}

		data := string(buf[:n])
		if strings.HasPrefix(data, "M-SEARCH * HTTP/1.1") {
			st := extractHeader(data, "ST")
			log.Printf("🔎 M-SEARCH from %s, ST=%s\n", src.String(), st)

			if st == "ssdp:all" ||
				st == serviceType ||
				strings.HasPrefix(st, "urn:schemas-upnp-org:service:") ||
				strings.HasPrefix(st, "urn:av-openhome-org:service:") ||
				strings.HasPrefix(st, "urn:bubblesoftapps-com:service:") {

				go sendSSDPResponse(src, usn, location, st)
			}
		}
	}
}

func sendSSDPResponse(dst *net.UDPAddr, usn, location, st string) {
	resp := fmt.Sprintf(
		"HTTP/1.1 200 OK\r\n"+
			"CACHE-CONTROL: max-age=1800\r\n"+
			"DATE: %s\r\n"+
			"EXT:\r\n"+
			"LOCATION: %s\r\n"+
			"SERVER: pmomusic/1.0 UPnP/1.1 DLNARenderer/1.0\r\n"+
			"ST: %s\r\n"+
			"USN: %s::%s\r\n"+
			"\r\n",
		time.Now().Format(time.RFC1123),
		location,
		st,
		usn,
		st,
	)

	// envoie UDP
	conn, err := net.DialUDP("udp4", nil, dst)
	if err != nil {
		log.Println("Failed to dial UDP to respond:", err)
		return
	}
	defer conn.Close()

	_, err = conn.Write([]byte(resp))
	if err != nil {
		log.Println("Failed to send SSDP response:", err)
	}
}

func extractHeader(data string, key string) string {
	lines := strings.Split(data, "\r\n")
	key = strings.ToLower(key)
	for _, line := range lines {
		parts := strings.SplitN(line, ":", 2)
		if len(parts) == 2 && strings.ToLower(strings.TrimSpace(parts[0])) == key {
			return strings.TrimSpace(parts[1])
		}
	}
	return ""
}
