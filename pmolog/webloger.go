package pmolog

import (
	"container/ring"
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"sync"
	"time"

	"github.com/sirupsen/logrus"
)

const bufferSize = 1000

// ---------- SSE Broker ----------

type SSEBroker struct {
	clients map[chan string]bool
	mu      sync.RWMutex
}

var (
	broker      = &SSEBroker{clients: make(map[chan string]bool)}
	logBuffer   = ring.New(bufferSize)
	bufferMutex sync.Mutex
)

// ---------- Hook for Logrus ----------

type SSELogHook struct{}

func (SSELogHook) Levels() []logrus.Level { return logrus.AllLevels }

func (SSELogHook) Fire(entry *logrus.Entry) error {
	msg := map[string]string{
		"time":    time.Now().Format(time.RFC3339),
		"level":   entry.Level.String(),
		"content": entry.Message,
	}
	b, _ := json.Marshal(msg)

	// add to buffer
	bufferMutex.Lock()
	logBuffer.Value = string(b)
	logBuffer = logBuffer.Next()
	bufferMutex.Unlock()

	// broadcast
	broker.mu.RLock()
	for ch := range broker.clients {
		select {
		case ch <- string(b):
		default: // skip if full
		}
	}
	broker.mu.RUnlock()
	return nil
}

// ---------- SSE Handler ----------

func sseHandler(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "text/event-stream")
	w.Header().Set("Cache-Control", "no-cache")
	w.Header().Set("Connection", "keep-alive")
	w.Header().Set("Access-Control-Allow-Origin", "*")

	flusher, ok := w.(http.Flusher)
	if !ok {
		http.Error(w, "Streaming unsupported", http.StatusInternalServerError)
		return
	}

	ch := make(chan string, 20)
	broker.mu.Lock()
	broker.clients[ch] = true
	broker.mu.Unlock()

	// replay buffer
	bufferMutex.Lock()
	logBuffer.Do(func(v interface{}) {
		if v != nil {
			fmt.Fprintf(w, "event: message\ndata: %s\n\n", v.(string))
		}
	})
	flusher.Flush()
	bufferMutex.Unlock()

	// stream new messages
	for {
		select {
		case msg := <-ch:
			fmt.Fprintf(w, "event: message\ndata: %s\n\n", msg)
			flusher.Flush()
		case <-r.Context().Done():
			broker.mu.Lock()
			delete(broker.clients, ch)
			broker.mu.Unlock()
			close(ch)
			return
		}
	}
}

// ---------- HTML Dashboard ----------

var indexHTML = `<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <title>🚀 Real-Time Logs</title>
  <link rel="stylesheet" href="https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.9.0/styles/github-dark.min.css">
  <style>
    body { background:#0d1117; color:#e6edf3; font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,Oxygen,Ubuntu,Cantarell,sans-serif; margin:0; }
    header { display:flex; align-items:center; justify-content:space-between; padding:10px 20px; background:#161b22; border-bottom:1px solid #30363d; }
    h1 { margin:0; font-size:20px; color:#58a6ff; }
    #controls { display:flex; gap:15px; align-items:center; }
    #logs { height:75vh; overflow-y:auto; background:#161b22; border:1px solid #30363d; border-radius:8px; padding:15px; margin:15px; }
    .log { margin:6px 0; padding:6px 10px; border-left:4px solid; border-radius:6px; }
    .log.error   { border-color:#f85149; background:rgba(248,81,73,0.1); }
    .log.warning { border-color:#d29922; background:rgba(210,153,34,0.1); }
    .log.info    { border-color:#58a6ff; background:rgba(56,139,253,0.1); }
    .log.debug   { border-color:#8957e5; background:rgba(137,87,229,0.1); }
    .time { color:#7d8590; font-size:12px; margin-right:10px; }
    input[type="checkbox"] { margin-right:4px; }
    #search { padding:4px 8px; border-radius:4px; border:none; }
    button { background:#238636; color:white; border:none; padding:6px 12px; border-radius:4px; cursor:pointer; }
    button:hover { background:#2ea043; }
  </style>
  <script src="https://cdn.jsdelivr.net/npm/marked/marked.min.js"></script>
  <script src="https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.9.0/highlight.min.js"></script>
</head>
<body>
  <header>
    <h1>📝 Logs en temps réel</h1>
    <div id="controls">
      <label><input type="checkbox" value="error" checked>❌ Error</label>
      <label><input type="checkbox" value="warning" checked>⚠️ Warning</label>
      <label><input type="checkbox" value="info" checked>ℹ️ Info</label>
      <label><input type="checkbox" value="debug" checked>🐛 Debug</label>
      <input id="search" type="text" placeholder="Search...">
      <label><input type="checkbox" id="autoscroll" checked> Auto-scroll</label>
      <button id="clear">Clear</button>
    </div>
  </header>
  <div id="logs"></div>
  <script>
    const logs=document.getElementById('logs');
    const es=new EventSource('/log-sse');
    const maxLogs=500;
    const filters={error:true,warning:true,info:true,debug:true};
    let searchTerm="";
    let autoScroll=true;

    marked.setOptions({ breaks:true, highlight: (code,lang)=>hljs.highlightAuto(code).value });

    es.addEventListener('message', e=>{
      const d=JSON.parse(e.data);
      if(!filters[d.level]) return;
      if(searchTerm && !d.content.toLowerCase().includes(searchTerm)) return;

      const line=document.createElement('div');
      line.className='log '+d.level;

      const t=document.createElement('span');
      t.className='time';
      t.textContent=new Date(d.time).toLocaleTimeString();
      line.appendChild(t);

      const c=document.createElement('div');
      c.innerHTML=marked.parse(d.content);
      line.appendChild(c);

      logs.appendChild(line);
      if(logs.children.length>maxLogs) logs.removeChild(logs.firstChild);
      if(autoScroll) logs.scrollTop=logs.scrollHeight;
    });

    // controls
    document.querySelectorAll('input[type=checkbox][value]').forEach(cb=>{
      cb.addEventListener('change',()=>{ filters[cb.value]=cb.checked; });
    });
    document.getElementById('search').addEventListener('input',e=>{
      searchTerm=e.target.value.toLowerCase();
    });
    document.getElementById('clear').addEventListener('click',()=>{ logs.innerHTML=""; });
    document.getElementById('autoscroll').addEventListener('change',e=>{ autoScroll=e.target.checked; });

    es.onerror=e=>console.error("SSE error",e);
  </script>
</body>
</html>`

// ---------- Handlers ----------

func indexHandler(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	fmt.Fprint(w, indexHTML)
}

// LoggerWeb installe les routes et arrête le broker quand ctx est annulé.
func LoggerWeb(ctx context.Context, mux *http.ServeMux) {
	logrus.SetFormatter(&logrus.TextFormatter{ForceColors: true, FullTimestamp: true})
	logrus.AddHook(SSELogHook{})

	mux.HandleFunc("/log", indexHandler)
	mux.HandleFunc("/log-sse", sseHandler)

	// goroutine d'arrêt
	go func() {
		<-ctx.Done()
		broker.mu.Lock()
		for ch := range broker.clients {
			close(ch)
			delete(broker.clients, ch)
		}
		broker.mu.Unlock()
		logrus.Info("Web logger stopped")
	}()

	logrus.Info("Web logger connected")
}
