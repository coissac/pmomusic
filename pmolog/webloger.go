package pmolog

import (
	"container/ring"
	"context"
	_ "embed"
	"encoding/json"
	"fmt"
	"net/http"
	"strings"
	"sync"
	"time"

	"github.com/sirupsen/logrus"
)

//go:embed index.html
var indexHTML string

const (
	bufferSize        = 1000
	clientChanSize    = 50
	heartbeatInterval = 15 * time.Second
)

// ---------- Enhanced SSE Broker ----------

type Client struct {
	messageChan chan string
	filters     map[string]bool
	searchTerm  string
}

type SSEBroker struct {
	clients map[*Client]bool
	mu      sync.RWMutex
}

var (
	broker      = &SSEBroker{clients: make(map[*Client]bool)}
	logBuffer   = ring.New(bufferSize)
	bufferMutex sync.RWMutex
)

// ---------- Enhanced Hook for Logrus ----------

type SSELogHook struct{}

func (SSELogHook) Levels() []logrus.Level { return logrus.AllLevels }

func (SSELogHook) Fire(entry *logrus.Entry) error {
	msg := map[string]interface{}{
		"time":    time.Now().Format(time.RFC3339Nano),
		"level":   entry.Level.String(),
		"content": entry.Message,
		"fields":  entry.Data,
	}
	b, _ := json.Marshal(msg)

	// Add to buffer
	bufferMutex.Lock()
	logBuffer.Value = string(b)
	logBuffer = logBuffer.Next()
	bufferMutex.Unlock()

	// Broadcast to clients
	broker.mu.RLock()
	for client := range broker.clients {
		// Apply client-side filtering before sending
		if !client.filters[strings.ToLower(msg["level"].(string))] {
			continue
		}

		if client.searchTerm != "" &&
			!strings.Contains(strings.ToLower(msg["content"].(string)), client.searchTerm) &&
			!strings.Contains(strings.ToLower(msg["level"].(string)), client.searchTerm) {
			continue
		}

		select {
		case client.messageChan <- string(b):
		default:
			// Skip if client channel is full (client is too slow)
		}
	}
	broker.mu.RUnlock()
	return nil
}

// ---------- Enhanced SSE Handler ----------

func sseHandler(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "text/event-stream")
	w.Header().Set("Cache-Control", "no-cache")
	w.Header().Set("Connection", "keep-alive")
	w.Header().Set("Access-Control-Allow-Origin", "*")
	w.Header().Set("X-Accel-Buffering", "no") // Disable buffering for nginx

	flusher, ok := w.(http.Flusher)
	if !ok {
		http.Error(w, "Streaming unsupported", http.StatusInternalServerError)
		return
	}

	// Parse query parameters for initial filters
	query := r.URL.Query()
	filters := map[string]bool{
		"error":   query.Get("error") != "false",
		"warning": query.Get("warning") != "false",
		"info":    query.Get("info") != "false",
		"debug":   query.Get("debug") != "false",
	}
	searchTerm := strings.ToLower(query.Get("search"))

	// Create client
	client := &Client{
		messageChan: make(chan string, clientChanSize),
		filters:     filters,
		searchTerm:  searchTerm,
	}

	// Register client
	broker.mu.Lock()
	broker.clients[client] = true
	broker.mu.Unlock()

	// Send initial heartbeat to prevent connection timeout
	fmt.Fprintf(w, "event: heartbeat\ndata: %s\n\n", time.Now().Format(time.RFC3339))
	flusher.Flush()

	// Replay buffer
	bufferMutex.RLock()
	logBuffer.Do(func(v interface{}) {
		if v != nil {
			// Apply filtering to historical messages
			var msg map[string]interface{}
			if err := json.Unmarshal([]byte(v.(string)), &msg); err == nil {
				if !filters[strings.ToLower(msg["level"].(string))] {
					return
				}

				if searchTerm != "" &&
					!strings.Contains(strings.ToLower(msg["content"].(string)), searchTerm) &&
					!strings.Contains(strings.ToLower(msg["level"].(string)), searchTerm) {
					return
				}

				fmt.Fprintf(w, "event: message\ndata: %s\n\n", v.(string))
			}
		}
	})
	flusher.Flush()
	bufferMutex.RUnlock()

	// Create a ticker for heartbeats
	heartbeat := time.NewTicker(heartbeatInterval)
	defer heartbeat.Stop()

	// Stream new messages
	for {
		select {
		case msg := <-client.messageChan:
			fmt.Fprintf(w, "event: message\ndata: %s\n\n", msg)
			flusher.Flush()
		case <-heartbeat.C:
			fmt.Fprintf(w, "event: heartbeat\ndata: %s\n\n", time.Now().Format(time.RFC3339))
			flusher.Flush()
		case <-r.Context().Done():
			broker.mu.Lock()
			delete(broker.clients, client)
			broker.mu.Unlock()
			close(client.messageChan)
			return
		}
	}
}

// ---------- Handlers ----------

func indexHandler(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	w.Header().Set("Cache-Control", "no-cache, no-store, must-revalidate")

	fmt.Fprint(w, indexHTML)
}

// LoggerWeb installe les routes et arrête le broker quand ctx est annulé.
func LoggerWeb(ctx context.Context, mux *http.ServeMux) {
	logrus.SetFormatter(&logrus.TextFormatter{
		ForceColors:   true,
		FullTimestamp: true,
	})
	logrus.AddHook(SSELogHook{})

	mux.HandleFunc("/log", indexHandler)
	mux.HandleFunc("/log-sse", sseHandler)

	// Goroutine d'arrêt
	go func() {
		<-ctx.Done()
		broker.mu.Lock()
		for client := range broker.clients {
			close(client.messageChan)
			delete(broker.clients, client)
		}
		broker.mu.Unlock()
		logrus.Info("Web logger stopped")
	}()

	logrus.Info("Web logger connected at /log")
}
