package soap

import (
	"bytes"
	"encoding/xml"
	"fmt"
)

// ----- Générateurs -----

// BuildUPnPResponse construit une réponse SOAP avec <ActionNameResponse>
func BuildUPnPResponse(serviceURN, action string, values map[string]string) ([]byte, error) {
	env := &Envelope{
		XMLName: xml.Name{Local: "s:Envelope"},
		Body: Body{
			Content: buildActionResponse(serviceURN, action, values),
		},
	}

	return marshalSOAP(env)
}

// BuildSOAPFault construit un Fault SOAP standard
func BuildSOAPFault(code, description, detail string) ([]byte, error) {
	env := &Envelope{
		XMLName: xml.Name{Local: "s:Envelope"},
		Body: Body{
			Content: buildFault(code, description, detail),
		},
	}

	return marshalSOAP(env)
}

// ----- Internes -----

func buildActionResponse(serviceURN, action string, values map[string]string) []byte {
	var buf bytes.Buffer
	buf.WriteString(fmt.Sprintf(`<u:%sResponse xmlns:u="%s">`, action, serviceURN))
	for k, v := range values {
		buf.WriteString(fmt.Sprintf("<%s>%s</%s>", k, xmlEscape(v), k))
	}
	buf.WriteString(fmt.Sprintf(`</u:%sResponse>`, action))
	return buf.Bytes()
}

func buildFault(code, description, detail string) []byte {
	return []byte(fmt.Sprintf(`
<Fault>
  <faultcode>%s</faultcode>
  <faultstring>%s</faultstring>
  <detail>%s</detail>
</Fault>`, xmlEscape(code), xmlEscape(description), xmlEscape(detail)))
}

func marshalSOAP(env *Envelope) ([]byte, error) {
	type soapEnvelope struct {
		XMLName xml.Name `xml:"s:Envelope"`
		SoapNS  string   `xml:"xmlns:s,attr"`
		EncNS   string   `xml:"s:encodingStyle,attr"`
		Body    struct {
			XMLName xml.Name `xml:"s:Body"`
			Content string   `xml:",innerxml"`
		}
	}

	tmp := soapEnvelope{
		SoapNS: "http://schemas.xmlsoap.org/soap/envelope/",
		EncNS:  "http://schemas.xmlsoap.org/soap/encoding/",
	}
	tmp.Body.Content = string(env.Body.Content)

	var buf bytes.Buffer
	buf.WriteString(`<?xml version="1.0" encoding="utf-8"?>`)
	enc := xml.NewEncoder(&buf)
	enc.Indent("", "  ")
	if err := enc.Encode(tmp); err != nil {
		return nil, err
	}
	enc.Flush()
	return buf.Bytes(), nil
}

// xmlEscape échappe manuellement les caractères dangereux
func xmlEscape(s string) string {
	var buf bytes.Buffer
	xml.EscapeText(&buf, []byte(s))
	return buf.String()
}
