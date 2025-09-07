package ssdp

import (
	"bufio"
	"context"
	"fmt"
	"net"
	"slices"
	"strings"
	"sync"
	"time"

	log "github.com/sirupsen/logrus"
)

const (
	SsdpAddr = "239.255.255.250"
	Port     = 1900
	MaxAge   = 1800
)

type Device struct {
	UUID       string
	DeviceType string
	Location   string
	Server     string
	NTs        []string
}

// GetNTs retourne la liste des NT à annoncer pour ce périphérique
func (d *Device) GetNTs() []string {
	return d.NTs
}

type SSDPServer struct {
	Devices map[string]*Device
	mu      sync.RWMutex
	conn    *net.UDPConn
}

// NewSSDPServer crée un serveur SSDP
func NewSSDPServer() *SSDPServer {
	return &SSDPServer{
		Devices: make(map[string]*Device),
	}
}

// AddDevice ajoute un périphérique et envoie un alive initial
func (s *SSDPServer) AddDevice(d *Device) {
	s.mu.Lock()
	defer s.mu.Unlock()

	s.Devices[d.UUID] = d
	for _, nt := range d.GetNTs() {
		s.SendAlive(d.UUID, nt, d.Location, d.Server)
	}
}

// RemoveDevice supprime un périphérique et envoie un byebye
func (s *SSDPServer) RemoveDevice(uuid string) {
	s.mu.Lock()
	defer s.mu.Unlock()

	d, ok := s.Devices[uuid]
	if !ok {
		return
	}
	for _, nt := range d.GetNTs() {
		s.SendByeBye(d.UUID, nt)
	}
	delete(s.Devices, uuid)
}

// Start démarre l'écoute SSDP et envoie les alive périodiques
func (s *SSDPServer) Start(ctx context.Context) error {
	addr := &net.UDPAddr{IP: net.ParseIP(SsdpAddr), Port: Port}
	log.Infof("✅ Starting SSDP listener")
	conn, err := net.ListenMulticastUDP("udp4", nil, addr)
	if err != nil {
		return err
	}
	conn.SetReadBuffer(8192)
	s.conn = conn

	// Alive périodique
	go func() {
		ticker := time.NewTicker(time.Duration(MaxAge/2) * time.Second)
		defer ticker.Stop()
		for {
			select {
			case <-ctx.Done():
				return
			case <-ticker.C:
				s.mu.RLock()
				for _, d := range s.Devices {
					for _, nt := range d.GetNTs() {
						s.SendAlive(d.UUID, nt, d.Location, d.Server)
					}
				}
				s.mu.RUnlock()
			}
		}
	}()

	// Écoute des M-SEARCH
	go func() {
		buf := make([]byte, 8192)
		for {
			select {
			case <-ctx.Done():
				log.Infof("✅ Stopping SSDP listener, sending byebye for all devices")
				s.mu.RLock()
				for _, d := range s.Devices {
					for _, nt := range d.GetNTs() {
						s.SendByeBye(d.UUID, nt)
					}
				}
				s.mu.RUnlock()
				conn.Close()
				return
			default:
				conn.SetReadDeadline(time.Now().Add(1 * time.Second))
				n, src, err := conn.ReadFromUDP(buf)
				if err != nil {
					if ne, ok := err.(net.Error); ok && ne.Timeout() {
						continue
					}
					log.Warnf("❌ SSDP read error: %v", err)
					continue
				}
				data := string(buf[:n])
				if strings.HasPrefix(data, "M-SEARCH") {
					s.mu.RLock()
					for _, d := range s.Devices {
						s.handleMSearch(src, data, d)
					}
					s.mu.RUnlock()
				}
			}
		}
	}()
	return nil
}

// SendSSDP envoie un NOTIFY multicast
func (s *SSDPServer) SendSSDP(msg string) error {
	addr := &net.UDPAddr{IP: net.ParseIP(SsdpAddr), Port: Port}
	_, err := s.conn.WriteToUDP([]byte(msg), addr)
	return err
}

// SendAlive envoie un NOTIFY ssdp:alive
func (s *SSDPServer) SendAlive(usn, nt, location, server string) {
	msg := fmt.Sprintf(`NOTIFY * HTTP/1.1
HOST: %s:%d
CACHE-CONTROL: max-age=%d
LOCATION: %s
NT: %s
NTS: ssdp:alive
SERVER: %s
USN: uuid:%s::%s

`, SsdpAddr, Port, MaxAge, location, nt, server, usn, nt)

	if err := s.SendSSDP(msg); err != nil {
		log.Warnf("❌ Failed to notify alive: USN %s: %v", usn, err)
	} else {
		log.Infof("✅ Notify alive: USN %s (NT=%s)", usn, nt)
	}
}

// SendByeBye envoie un NOTIFY ssdp:byebye
func (s *SSDPServer) SendByeBye(usn, nt string) {
	msg := fmt.Sprintf(`NOTIFY * HTTP/1.1
HOST: %s:%d
NT: %s
NTS: ssdp:byebye
USN: uuid:%s::%s

`, SsdpAddr, Port, nt, usn, nt)
	msg = strings.ReplaceAll(msg, "\n", "\r\n")

	if err := s.SendSSDP(msg); err != nil {
		log.Warnf("❌ Failed to notify byebye: USN %s: %v", usn, err)
	} else {
		log.Infof("👋 Notify byebye: USN %s (NT=%s)", usn, nt)
	}
}

// handleMSearch répond à un M-SEARCH en unicast
func (s *SSDPServer) handleMSearch(src *net.UDPAddr, req string, d *Device) {
	st := parseST(req)
	if st == "" {
		return
	}

	valid := st == "ssdp:all" ||
		slices.Contains(d.GetNTs(), st)

	if !valid {
		return
	}

	log.Infof("M-Search response on a valid ST: %s", st)
	nts := []string{st}
	if st == "ssdp:all" {
		nts = d.GetNTs()
	}

	nts = d.GetNTs()
	for _, st := range nts {
		resp := fmt.Sprintf(`HTTP/1.1 200 OK
CACHE-CONTROL: max-age=%d
DATE: %s
EXT:
LOCATION: %s
SERVER: %s
ST: %s
USN: uuid:%s::%s

`, MaxAge, time.Now().UTC().Format(time.RFC1123), d.Location, d.Server, st, d.UUID, st)
		resp = strings.ReplaceAll(resp, "\n", "\r\n")
		if _, err := s.conn.WriteToUDP([]byte(resp), src); err != nil {
			log.Warnf("❌ Failed to send M-SEARCH response to %v: %v", src, err)
		} else {
			log.Infof("📡 Responded to M-SEARCH from %v with ST=%s\n<details>\n\n```\n%s\n```\n</details>\n\n", src, st, resp)
		}
	}
}

// parseST extrait le ST d’un M-SEARCH
func parseST(req string) string {
	scanner := bufio.NewScanner(strings.NewReader(req))
	for scanner.Scan() {
		line := scanner.Text()
		if strings.HasPrefix(strings.ToUpper(line), "ST:") {
			st := strings.TrimSpace(line[3:])
			log.Infof("✅ Found ST=%s in M-SEARCH response", st)
			return st
		}
	}
	return ""
}
