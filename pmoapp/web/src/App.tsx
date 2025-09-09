// App.tsx
import { useEffect, useState } from "react";
import { Logs, type LogEntry } from "./Logs";
import "./App.css";

export default function App() {
  const [theme, setTheme] = useState<"light" | "dark">("dark");
  const [logs, setLogs] = useState<LogEntry[]>([]);

  const toggleTheme = () =>
    setTheme((prev) => (prev === "dark" ? "light" : "dark"));

  // Fonction pour transformer le SSE en LogEntry
  const mapSSEToLogEntry = (sseData: any, index: number): LogEntry => ({
    id: sseData.time || `log-${index}`,
    level: sseData.level || "info",
    message: sseData.content || "",
  });

  useEffect(() => {
    const evtSource = new EventSource("/log-sse");
    let idx = 0;

    evtSource.addEventListener("message", (e: MessageEvent) => {
      try {
        const data = JSON.parse(e.data);
        const newLog = mapSSEToLogEntry(data, idx++);
        setLogs((prev) => [...prev, newLog]);
      } catch (err) {
        console.error("Erreur parsing SSE:", err);
      }
    });

    return () => {
      evtSource.close();
    };
  }, []);

  return (
    <div className={theme}>
      <header className="flex items-center justify-between p-4 bg-neutral-800">
        <h1 className="text-xl font-bold">PMO-Music logs</h1>
        <button
          className="px-4 py-2 bg-blue-600 rounded hover:bg-blue-500"
          onClick={toggleTheme}
        >
          Theme: {theme}
        </button>
      </header>
      <Logs logs={logs} theme={theme} />
    </div>
  );
}
