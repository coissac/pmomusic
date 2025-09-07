package pmolog

import (
	"container/ring"
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

// Buffer circulaire pour stocker les 1000 derniers messages
var (
	logBuffer   = ring.New(1000)
	bufferMutex sync.Mutex
)

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
	// Formater le log avec le niveau et le message
	logLine := fmt.Sprintf("[%s] %s", entry.Level.String(), entry.Message)

	// Ajouter le message au buffer circulaire
	bufferMutex.Lock()
	logBuffer.Value = logLine
	logBuffer = logBuffer.Next()
	bufferMutex.Unlock()

	// Envoyer le log à tous les clients connectés
	broker.mutex.Lock()
	for client := range broker.clients {
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

	// Envoyer d'abord les 1000 derniers messages stockés
	bufferMutex.Lock()
	logBuffer.Do(func(value interface{}) {
		if value != nil {
			if msg, ok := value.(string); ok {
				// Déterminer le niveau de log pour le style CSS
				level := "info"
				if len(msg) > 7 {
					switch msg[1:6] {
					case "ERROR":
						level = "error"
					case "WARNI":
						level = "warning"
					case "DEBUG":
						level = "debug"
					}
				}

				// Formater le message en JSON pour inclure le niveau
				jsonMsg := fmt.Sprintf("{\"content\": \"%s\", \"level\": \"%s\"}", escapeJSONString(msg), level)
				fmt.Fprintf(w, "event: message\ndata: %s\n\n", jsonMsg)
			}
		}
	})
	w.(http.Flusher).Flush()
	bufferMutex.Unlock()

	// Envoyer les logs au client au fur et à mesure
	for {
		select {
		case msg := <-messageChan:
			// Déterminer le niveau de log pour le style CSS
			level := "info"
			if len(msg) > 7 {
				switch msg[1:6] {
				case "ERROR":
					level = "error"
				case "WARNI":
					level = "warning"
				case "DEBUG":
					level = "debug"
				}
			}

			// Formater le message en JSON pour inclure le niveau
			jsonMsg := fmt.Sprintf("{\"content\": \"%s\", \"level\": \"%s\"}", escapeJSONString(msg), level)
			fmt.Fprintf(w, "event: message\ndata: %s\n\n", jsonMsg)
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

// Fonction pour échapper les chaînes JSON
func escapeJSONString(s string) string {
	// Échapper les guillemets et les antislashes
	escaped := ""
	for _, c := range s {
		switch c {
		case '"':
			escaped += "\\\""
		case '\\':
			escaped += "\\\\"
		case '\n':
			escaped += "\\n"
		case '\r':
			escaped += "\\r"
		case '\t':
			escaped += "\\t"
		default:
			escaped += string(c)
		}
	}
	return escaped
}

// Page HTML pour afficher les logs avec support Markdown
var indexHTML = `
<!DOCTYPE html>
<html>
<head>
    <title>Logs en temps réel</title>
    <style>
        body { 
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell, sans-serif; 
            background: #0d1117; 
            color: #e6edf3; 
            margin: 0; 
            padding: 20px; 
        }
        .container {
            max-width: 1200px;
            margin: 0 auto;
        }
        h1 {
            color: #58a6ff;
            border-bottom: 1px solid #30363d;
            padding-bottom: 10px;
        }
        #logs {
            background: #161b22;
            border: 1px solid #30363d;
            border-radius: 6px;
            padding: 15px;
            height: 70vh;
            overflow-y: auto;
            font-size: 14px;
            line-height: 1.5;
        }
        .log-line {
            margin: 8px 0;
            padding: 8px 12px;
            border-radius: 6px;
            border-left: 4px solid #58a6ff;
        }
        .log-line.error {
            border-left-color: #f85149;
            background-color: rgba(248, 81, 73, 0.1);
        }
        .log-line.warning {
            border-left-color: #d29922;
            background-color: rgba(210, 153, 34, 0.1);
        }
        .log-line.info {
            border-left-color: #58a6ff;
            background-color: rgba(56, 139, 253, 0.1);
        }
        .log-line.debug {
            border-left-color: #8957e5;
            background-color: rgba(137, 87, 229, 0.1);
        }
        .timestamp {
            color: #7d8590;
            font-size: 12px;
            margin-right: 10px;
        }
        /* Styles Markdown */
        .markdown-body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell, sans-serif;
        }
        .markdown-body code {
            background-color: rgba(240, 246, 252, 0.15);
            border-radius: 6px;
            padding: 0.2em 0.4em;
            font-family: ui-monospace, SFMono-Regular, SF Mono, Consolas, Liberation Mono, Menlo, monospace;
        }
        .markdown-body pre {
            background-color: rgba(240, 246, 252, 0.15);
            border-radius: 6px;
            padding: 16px;
            overflow: auto;
        }
        .markdown-body pre code {
            background: none;
            padding: 0;
        }
        .markdown-body blockquote {
            border-left: 4px solid #30363d;
            padding-left: 16px;
            margin-left: 0;
            color: #7d8590;
        }
        .markdown-body a {
            color: #58a6ff;
            text-decoration: none;
        }
        .markdown-body a:hover {
            text-decoration: underline;
        }
    </style>
    <!-- Marked.js pour le rendu Markdown -->
    <script src="https://cdn.jsdelivr.net/npm/marked/marked.min.js"></script>
</head>
<body>
    <div class="container">
        <h1>📝 Logs en temps réel (1000 derniers messages)</h1>
        <div id="logs"></div>
    </div>
    
    <script>
        const eventSource = new EventSource('/log-sse');
        const logsContainer = document.getElementById('logs');
        
        // Configuration de Marked.js
        marked.setOptions({
            breaks: true,
            highlight: function(code, lang) {
                // Simplement retourner le code non highlighté pour l'instant
                return code;
            }
        });
        
        // Fonction pour ajouter un message aux logs
        function addLogMessage(data) {
            const logLine = document.createElement('div');
            logLine.className = 'log-line ' + data.level;
            
            // Ajouter un timestamp
            const timestamp = new Date().toLocaleTimeString();
            const timestampSpan = document.createElement('span');
            timestampSpan.className = 'timestamp';
            timestampSpan.textContent = timestamp;
            logLine.appendChild(timestampSpan);
            
            // Traiter le contenu Markdown
            const contentDiv = document.createElement('div');
            contentDiv.className = 'markdown-body';
            contentDiv.innerHTML = marked.parse(data.content);
            logLine.appendChild(contentDiv);
            
            logsContainer.appendChild(logLine);
            
            // Défilement automatique
            logsContainer.scrollTop = logsContainer.scrollHeight;
        }
        
        eventSource.addEventListener('message', function(event) {
            const data = JSON.parse(event.data);
            addLogMessage(data);
        });
        
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
	// Configurer Logrus pour le développement
	logrus.SetFormatter(&logrus.TextFormatter{
		ForceColors:   true,
		FullTimestamp: true,
	})

	// Ajouter le hook SSE à Logrus
	logrus.AddHook(&SSELogHook{})

	mux.HandleFunc("/log", indexHandler)
	mux.HandleFunc("/log-sse", sseHandler)

	logrus.Info("Web logger connected")

}
