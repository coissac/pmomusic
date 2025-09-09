package upnp

import (
	"fmt"
	"html"
	"net/http"
)

func (s *Server) ServeDebugIndex(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "text/html; charset=utf-8")

	fmt.Fprintf(w, `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>UPnP Debug Interface</title>
  <style>
    body { font-family: sans-serif; margin: 2em; }
    h1 { border-bottom: 1px solid #ccc; }
    pre { background: #f5f5f5; padding: 1em; overflow-x: auto; }
    a { color: #007bff; text-decoration: none; }
    a:hover { text-decoration: underline; }
  </style>
</head>
<body>
  <h1>Host %s </h1>
  <h2>address: %s</h2>`,
		s.Name(),
		html.EscapeString(html.EscapeString(s.BaseURL())))

	fmt.Fprint(w, `
</body>
</html>
`)
}
