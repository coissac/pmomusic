package netutils

import (
	"errors"
	"net"
	"sort"
	"strings"
)

// GuessLocalIP returns the best-guess local IP address usable for UPnP location,
// in order of preference: eth0 > en* > wl* > any private, non-loopback IP.
func GuessLocalIP() (string, error) {
	ifaces, err := net.Interfaces()
	if err != nil {
		return "", err
	}

	type scoredIP struct {
		ip    net.IP
		score int
	}

	var candidates []scoredIP

	for _, iface := range ifaces {
		// Skip interfaces that are down or loopback
		if iface.Flags&net.FlagUp == 0 || iface.Flags&net.FlagLoopback != 0 {
			continue
		}

		addrs, err := iface.Addrs()
		if err != nil {
			continue
		}

		for _, addr := range addrs {
			var ip net.IP

			switch v := addr.(type) {
			case *net.IPNet:
				ip = v.IP
			case *net.IPAddr:
				ip = v.IP
			}

			// Skip non-IPv4 or non-private addresses
			if ip == nil || ip.IsLoopback() || ip.To4() == nil || !isPrivateIPv4(ip) {
				continue
			}

			score := scoreInterfaceName(iface.Name)
			candidates = append(candidates, scoredIP{ip: ip, score: score})
		}
	}

	if len(candidates) == 0 {
		return "", errors.New("no suitable local IP found")
	}

	// Prefer interfaces with higher score
	sort.SliceStable(candidates, func(i, j int) bool {
		return candidates[i].score > candidates[j].score
	})

	return candidates[0].ip.String(), nil
}

func isPrivateIPv4(ip net.IP) bool {
	private := []string{
		"10.", "172.16.", "172.17.", "172.18.", "172.19.", "172.2", "192.168.",
	}
	ipStr := ip.String()
	for _, p := range private {
		if strings.HasPrefix(ipStr, p) {
			return true
		}
	}
	return false
}

func scoreInterfaceName(name string) int {
	switch {
	case name == "eth0":
		return 100
	case strings.HasPrefix(name, "en"):
		return 80
	case name == "wlan0" || strings.HasPrefix(name, "wl"):
		return 60
	default:
		return 10
	}
}
