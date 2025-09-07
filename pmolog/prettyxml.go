package pmolog

import (
	"bytes"
	"encoding/xml"
)

func PrettyPrintXML(raw string) string {
	var out bytes.Buffer
	dec := xml.NewDecoder(bytes.NewReader([]byte(raw)))
	enc := xml.NewEncoder(&out)
	enc.Indent("", "  ")
	for {
		t, err := dec.Token()
		if err != nil {
			break
		}
		if err := enc.EncodeToken(t); err != nil {
			break
		}
	}
	enc.Flush()
	return out.String()
}
