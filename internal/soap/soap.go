package soap

import (
	"bytes"
	"encoding/xml"
	"io"
	"log"
	"net/http"
)

func HandleSOAP(w http.ResponseWriter, r *http.Request) {
	body, err := io.ReadAll(r.Body)
	if err != nil {
		log.Println("Failed to read SOAP request:", err)
		http.Error(w, "Invalid request", http.StatusBadRequest)
		return
	}
	defer r.Body.Close()

	log.Println("Received SOAP request:")
	log.Println(string(body))

	// Extract action name from the SOAP body
	action := extractSOAPAction(body)
	log.Printf("SOAP Action: %s\n", action)

	// Send dummy response
	w.Header().Set("Content-Type", "text/xml; charset=\"utf-8\"")
	w.WriteHeader(http.StatusOK)
	w.Write([]byte(dummySOAPResponse(action)))
}

func extractSOAPAction(body []byte) string {
	type Envelope struct {
		Body struct {
			XMLName xml.Name
		} `xml:"Body"`
	}

	var env Envelope
	if err := xml.Unmarshal(body, &env); err != nil {
		log.Println("SOAP parse error:", err)
		return "UnknownAction"
	}

	decoder := xml.NewDecoder(bytes.NewReader(body))
	for {
		tok, err := decoder.Token()
		if err != nil {
			break
		}
		if se, ok := tok.(xml.StartElement); ok && se.Name.Local != "Envelope" && se.Name.Local != "Body" {
			return se.Name.Local
		}
	}
	return "UnknownAction"
}

func dummySOAPResponse(action string) string {
	// You can specialize per action if needed.
	return `<?xml version="1.0"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" 
            s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
  <s:Body>
    <u:` + action + `Response xmlns:u="urn:schemas-upnp-org:service:AVTransport:1">
    </u:` + action + `Response>
  </s:Body>
</s:Envelope>`
}
