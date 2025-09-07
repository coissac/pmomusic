package soap

import (
	"bytes"
	"encoding/xml"
	"io"

	"gargoton.petite-maison-orange.fr/eric/pmomusic/didl"
	log "github.com/sirupsen/logrus"
)

type Envelope struct {
	XMLName xml.Name `xml:"Envelope"`
	Body    Body     `xml:"Body"`
}

type Body struct {
	Content []byte `xml:",innerxml"` // <- capture tout le contenu du Body sous forme de XML brut
}

func prettyPrintXML(raw string) string {
	var out bytes.Buffer
	decoder := xml.NewDecoder(bytes.NewReader([]byte(raw)))
	encoder := xml.NewEncoder(&out)
	encoder.Indent("", "  ") // définit l'indentation
	for {
		t, err := decoder.Token()
		if err != nil {
			break
		}
		if err := encoder.EncodeToken(t); err != nil {
			break
		}
	}
	encoder.Flush()
	return out.String()
}

func ParseSOAPGeneric(body []byte) {
	var env Envelope
	if err := xml.Unmarshal(body, &env); err != nil {
		log.Warnf("❌ Failed to unmarshal SOAP Envelope: %v", err)
		return
	}

	decoder := xml.NewDecoder(bytes.NewReader(env.Body.Content))
	var currentAction string
	args := make(map[string]interface{})

	for {
		tok, err := decoder.Token()
		if err != nil {
			if err != io.EOF {
				log.Warnf("❌ SOAP parse error: %v", err)
			}
			break
		}

		switch t := tok.(type) {
		case xml.StartElement:
			if currentAction == "" {
				currentAction = t.Name.Local // nom de l'action
			} else {
				var value string
				decoder.DecodeElement(&value, &t)

				var ival interface{}
				ival, err = didl.Parse(value)

				if err == nil {
					ival = ival.(*didl.DIDLLite).ToMarkdown()
				} else {
					ival = prettyPrintXML(value)
				}

				args[t.Name.Local] = ival

			}
		}
	}

	log.Infof("📡 SOAP Action: %s", currentAction)
	for k, v := range args {
		log.Infof("    %s = %v", k, v)
	}
}
