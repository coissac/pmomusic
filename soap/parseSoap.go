package soap

import (
	"bytes"
	"encoding/xml"
	"fmt"
	"io"

	log "github.com/sirupsen/logrus"
)

// ----- SOAP envelope -----

type Envelope struct {
	XMLName xml.Name `xml:"http://schemas.xmlsoap.org/soap/envelope/ Envelope"`
	Header  *Header  `xml:"Header"`
	Body    Body     `xml:"Body"`
}

type Header struct {
	Content []byte `xml:",innerxml"`
}

type Body struct {
	Content []byte `xml:",innerxml"`
}

// ----- UPnP request/response -----

type ActionRequest struct {
	Name   string
	Args   map[string]interface{}
	RawXML []byte
}

type ActionResponse struct {
	Name   string
	Values map[string]string
	RawXML []byte
}

type Fault struct {
	Code        string
	Description string
	Detail      string
	RawXML      []byte
}

// ----- Utils -----

// ----- Parseurs -----

func ParseSOAPEnvelope(body []byte) (*Envelope, error) {
	var env Envelope
	if err := xml.Unmarshal(body, &env); err != nil {
		return nil, fmt.Errorf("unmarshal SOAP Envelope: %w", err)
	}
	return &env, nil
}

// ParamDecoder permet de transformer les valeurs des paramètres et éventuellement renommer le paramètre
type ParamDecoder func(action, param, value string) (newParam string, out interface{}, err error)

// ParseUPnPAction extrait l’action et ses arguments à partir d’un Body SOAP.
// Si decoder != nil, il est appelé pour chaque paramètre.
func ParseUPnPAction(env *Envelope, decoder ParamDecoder) (*ActionRequest, error) {
	dec := xml.NewDecoder(bytes.NewReader(env.Body.Content))
	var currentAction string
	args := make(map[string]interface{})

	for {
		tok, err := dec.Token()
		if err != nil {
			if err == io.EOF {
				break
			}
			return nil, fmt.Errorf("SOAP parse error: %w", err)
		}

		switch t := tok.(type) {
		case xml.StartElement:
			if currentAction == "" {
				currentAction = t.Name.Local
			} else {
				var value string
				if err := dec.DecodeElement(&value, &t); err != nil {
					return nil, fmt.Errorf("decode param %s: %w", t.Name.Local, err)
				}

				paramName := t.Name.Local
				var paramValue interface{} = value

				if decoder != nil {
					if newName, out, err := decoder(currentAction, paramName, value); err == nil {
						paramName = newName
						paramValue = out
					}
				}

				args[paramName] = paramValue
			}
		}
	}

	return &ActionRequest{
		Name:   currentAction,
		Args:   args,
		RawXML: env.Body.Content,
	}, nil
}

// Response (renderer -> contrôleur)
func ParseUPnPResponse(env *Envelope) (*ActionResponse, *Fault, error) {
	dec := xml.NewDecoder(bytes.NewReader(env.Body.Content))
	var respName string
	values := make(map[string]string)

	for {
		tok, err := dec.Token()
		if err != nil {
			if err == io.EOF {
				break
			}
			return nil, nil, fmt.Errorf("SOAP parse error: %w", err)
		}

		switch t := tok.(type) {
		case xml.StartElement:
			if t.Name.Local == "Fault" {
				// Parse fautes SOAP
				var f struct {
					Code   string `xml:"faultcode"`
					Desc   string `xml:"faultstring"`
					Detail string `xml:"detail"`
				}
				if err := dec.DecodeElement(&f, &t); err != nil {
					return nil, nil, fmt.Errorf("decode Fault: %w", err)
				}
				return nil, &Fault{
					Code:        f.Code,
					Description: f.Desc,
					Detail:      f.Detail,
					RawXML:      env.Body.Content,
				}, nil
			}

			if respName == "" {
				respName = t.Name.Local
			} else {
				var value string
				if err := dec.DecodeElement(&value, &t); err != nil {
					return nil, nil, fmt.Errorf("decode response param %s: %w", t.Name.Local, err)
				}
				values[t.Name.Local] = value
			}
		}
	}

	if respName == "" {
		return nil, nil, fmt.Errorf("no response or fault in SOAP body")
	}

	return &ActionResponse{respName, values, env.Body.Content}, nil, nil
}

// ----- Exemple -----

func ParseSOAPGeneric(body []byte, decoder ParamDecoder) {
	env, err := ParseSOAPEnvelope(body)
	if err != nil {
		log.Warnf("❌ %v", err)
		return
	}

	// Essayer d'abord comme requête
	if req, err := ParseUPnPAction(env, decoder); err == nil && req.Name != "" {
		log.Infof("📡 SOAP Request Action: %s", req.Name)
		for k, v := range req.Args {
			log.Infof("    %s = %v", k, v)
		}
		return
	}

	// Sinon comme réponse
	if resp, fault, err := ParseUPnPResponse(env); err == nil {
		if resp != nil {
			log.Infof("📡 SOAP Response: %s", resp.Name)
			for k, v := range resp.Values {
				log.Infof("    %s = %v", k, v)
			}
		} else if fault != nil {
			log.Warnf("❌ SOAP Fault: %s - %s (detail: %s)", fault.Code, fault.Description, fault.Detail)
		}
	}
}
