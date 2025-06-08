package soap

import (
	"io"
	"log"
	"net/http"
)

func HandleSOAP(w http.ResponseWriter, r *http.Request) {
	action := r.Header.Get("SOAPACTION")
	body, _ := io.ReadAll(r.Body)

	log.Println("----------")
	log.Println("SOAPAction:", action)
	log.Println("Body:\n", string(body))

	w.WriteHeader(http.StatusOK)
}
