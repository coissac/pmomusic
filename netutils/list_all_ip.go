package netutils

import (
	"net"
)

// ListAllIPs returns a map of interface names to their associated IPv4 addresses.
func ListAllIPs() map[string][]string {
	result := make(map[string][]string)

	ifaces, err := net.Interfaces()
	if err != nil {
		result["error"] = []string{err.Error()}
		return result
	}

	for _, iface := range ifaces {
		if iface.Flags&net.FlagUp == 0 {
			continue // Ignore down interfaces
		}

		addrs, err := iface.Addrs()
		if err != nil {
			continue
		}

		var ips []string
		for _, addr := range addrs {
			var ip net.IP
			switch v := addr.(type) {
			case *net.IPNet:
				ip = v.IP
			case *net.IPAddr:
				ip = v.IP
			}

			if ip == nil || ip.To4() == nil || ip.IsLoopback() {
				continue
			}
			ips = append(ips, ip.String())
		}

		if len(ips) > 0 {
			result[iface.Name] = ips
		}
	}

	return result
}
