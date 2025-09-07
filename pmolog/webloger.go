package pmolog

import (
	"fmt"
	"net/http"
	"sync"

	"github.com/sirupsen/logrus"
)

// Structure pour gérer les connexions SSE
type SSEBroker struct {
	clients map[chan string]bool
	mutex   sync.Mutex
}

// Initialiser le broker SSE
var broker = &SSEBroker{
	clients: make(map[chan string]bool),
}

// Hook personnalisé pour Logrus qui envoie les logs aux clients SSE
type SSELogHook struct{}

func (hook *SSELogHook) Levels() []logrus.Level {
	return logrus.AllLevels
}

func (hook *SSELogHook) Fire(entry *logrus.Entry) error {
	// Formater le log
	logLine := fmt.Sprintf("[%s] %s", entry.Level.String(), entry.Message)

	// Envoyer le log à tous les clients connectés
	broker.mutex.Lock()
	for client := range broker.clients {
		// Non-bloquant pour éviter qu'un client lent ne bloque tout
		select {
		case client <- logLine:
		default:
			// Client saturé, on skip
		}
	}
	broker.mutex.Unlock()

	return nil
}

// Handler SSE qui envoie les logs en temps réel
func sseHandler(w http.ResponseWriter, r *http.Request) {
	// Configurer les en-têtes SSE
	w.Header().Set("Content-Type", "text/event-stream")
	w.Header().Set("Cache-Control", "no-cache")
	w.Header().Set("Connection", "keep-alive")
	w.Header().Set("Access-Control-Allow-Origin", "*")

	// Créer un canal pour ce client
	messageChan := make(chan string, 10)

	// Ajouter le client au broker
	broker.mutex.Lock()
	broker.clients[messageChan] = true
	broker.mutex.Unlock()

	// Envoyer un message de bienvenue
	fmt.Fprintf(w, "data: %s\n\n", "Connexion établie. Attente des logs...")
	w.(http.Flusher).Flush()

	// Envoyer les logs au client au fur et à mesure
	for {
		select {
		case msg := <-messageChan:
			fmt.Fprintf(w, "data: %s\n\n", msg)
			w.(http.Flusher).Flush()
		case <-r.Context().Done():
			// Supprimer le client quand la connexion est fermée
			broker.mutex.Lock()
			delete(broker.clients, messageChan)
			broker.mutex.Unlock()
			close(messageChan)
			return
		}
	}
}

// Page HTML pour afficher les logs
var indexHTML = `
<!DOCTYPE html>
<html>
<head>
    <title>Logs en temps réel</title>
    <style>
        body { font-family: monospace; background: #000; color: #0f0; }
        .log-line { margin: 2px 0; }
        .error { color: #f00; }
        .warning { color: #ff0; }
        .info { color: #0f0; }
        .debug { color: #0af; }
    </style>
</head>
<body>
    <h1>Logs en temps réel</h1>
    <div id="logs"></div>
    
    <script>
        const eventSource = new EventSource('/log-sse');
        const logsContainer = document.getElementById('logs');
        
        eventSource.onmessage = function(event) {
            const logLine = document.createElement('div');
            logLine.className = 'log-line';
            logLine.textContent = event.data;
            
            // Ajouter des classes CSS en fonction du niveau de log
            if (event.data.includes('[error]')) logLine.classList.add('error');
            else if (event.data.includes('[warning]')) logLine.classList.add('warning');
            else if (event.data.includes('[info]')) logLine.classList.add('info');
            else if (event.data.includes('[debug]')) logLine.classList.add('debug');
            
            logsContainer.appendChild(logLine);
            logsContainer.scrollTop = logsContainer.scrollHeight;
        };
        
        eventSource.onerror = function(error) {
            console.error('Erreur SSE:', error);
        };
    </script>
</body>
</html>
`

// Handler pour servir la page HTML
func indexHandler(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	fmt.Fprint(w, indexHTML)
}

func LoggerWeb(mux *http.ServeMux) {
	// Ajouter le hook SSE à Logrus
	logrus.AddHook(&SSELogHook{})

	// Configurer Logrus pour le développement
	logrus.SetFormatter(&logrus.TextFormatter{
		ForceColors:   true,
		FullTimestamp: true,
	})

	mux.HandleFunc("/log", indexHandler)
	mux.HandleFunc("/log-sse", sseHandler)

	logrus.Info("Web logger connected")

}
