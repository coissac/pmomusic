package upnp

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"net/http"
	"sync"
	"time"

	"github.com/beevik/etree"
	log "github.com/sirupsen/logrus"

	"gargoton.petite-maison-orange.fr/eric/pmomusic/netutils"
)

type Server struct {
	name     string
	HTTPPort int
	baseURL  string

	Logger  *log.Logger
	httpSrv *http.Server

	devices   DeviceInstanceSet
	mu        sync.RWMutex
	startOnce sync.Once
	stopOnce  sync.Once
}

func NewServer(name string, opts ...ServerOption) *Server {
	config := GetConfig()

	baseURL := config.GetBaseURL()
	httpPort := config.GetHTTPPort()

	if baseURL == "" {
		ip, err := netutils.GuessLocalIP()
		if err != nil {
			panic(fmt.Errorf("unable to determine local IP: %w", err))
		}
		baseURL = fmt.Sprintf("http://%s:%d", ip, httpPort)
	}

	s := &Server{
		name:     name,
		HTTPPort: httpPort,
		baseURL:  baseURL,
		Logger:   log.New(),
	}

	for _, opt := range opts {
		opt(s)
	}

	return s
}

func (s *Server) Name() string   { return s.name }
func (s *Server) TypeID() string { return "Server" }

type ServerOption func(*Server)

func WithLogger(l *log.Logger) ServerOption {
	return func(s *Server) {
		s.Logger = l
	}
}

func (s *Server) Start() error {
	s.startOnce.Do(func() {
		mux := http.NewServeMux()

		s.mu.RLock()

		mux.HandleFunc("/", s.ServeDebugIndex)

		s.httpSrv = &http.Server{
			Addr:    fmt.Sprintf(":%d", s.HTTPPort),
			Handler: mux,
		}

		for device := range s.devices.All() {
			err := device.RegisterURLs()

			if err != nil {
				log.Panicf("❌ Cannot register URLs: %v", err)
			}
		}

		s.mu.RUnlock()

		go func() {
			if err := s.httpSrv.ListenAndServe(); err != nil && !errors.Is(err, http.ErrServerClosed) {
				s.Logger.Printf("❌ server error: %v", err)
			}
		}()

		log.Infof("✅ UPnP server started on %s", s.baseURL)
	})

	return nil
}

func (s *Server) Stop(ctx context.Context) error {
	var err error
	s.stopOnce.Do(func() {
		if s.httpSrv != nil {
			s.Logger.Println("✅ Shutting down UPNP server...")
			err = s.httpSrv.Shutdown(ctx)
		}
	})
	return err
}

func (s *Server) Run(ctx context.Context) error {
	if err := s.Start(); err != nil {
		return fmt.Errorf("failed to start server: %w", err)
	}

	// attente d’annulation du contexte
	<-ctx.Done()

	// arrêt avec le même ctx ou un nouveau ctx avec timeout
	shutdownCtx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	return s.Stop(shutdownCtx)
}

func (s *Server) BaseURL() string { return s.baseURL }

// ServeXML prend un générateur de XML (*etree.Element)
// et renvoie la string XML avec header.
func (s *Server) XML(gen func() *etree.Element) (string, error) {
	root := gen()
	doc := etree.NewDocument()
	doc.SetRoot(root)

	doc.Indent(2)

	buf := new(bytes.Buffer)
	if _, err := doc.WriteTo(buf); err != nil {
		return "", err
	}

	// Ajoute le header XML
	return `<?xml version="1.0" encoding="utf-8"?>` + "\n" + buf.String(), nil
}

func (s *Server) ServeXML(gen func() *etree.Element) func(w http.ResponseWriter, r *http.Request) {

	return func(w http.ResponseWriter, r *http.Request) {
		xmlStr, err := s.XML(gen)
		if err != nil {
			http.Error(w, "failed to generate XML", http.StatusInternalServerError)
			return
		}
		w.Header().Set("Content-Type", `text/xml; charset="utf-8"`)
		w.WriteHeader(http.StatusOK)
		w.Write([]byte(xmlStr))
	}
}
