// Logs.tsx
import ReactMarkdown from "react-markdown";
import rehypeRaw from "rehype-raw";

export type LogEntry = {
  id: string;
  level: "info" | "warn" | "error" | "debug";
  message: string;
};

type LogsProps = {
  logs: LogEntry[];
  theme: "light" | "dark";
};

export function Logs({ logs, theme }: LogsProps) {
  const levelClass = (level: LogEntry["level"]) => {
    switch (level) {
      case "info":
        return "info";
      case "warn":
        return "warning";
      case "error":
        return "error";
      case "debug":
        return "debug";
      default:
        return "info";
    }
  };

  const decodeMessage = (msg: string) => {
    const txt = document.createElement("textarea");
    txt.innerHTML = msg;
    return txt.value;
  };

  return (
    <div className={`log-container ${theme}`}>
      {logs.map((log) => (
        <div key={log.id} className={`log-entry ${levelClass(log.level)}`}>
          <ReactMarkdown
            rehypePlugins={[rehypeRaw]}
            children={decodeMessage(log.message)}
            components={{
              code({ node, className, children, ...props }) {
                return (
                  <code
                    className={`bg-gray-700 text-white px-1 py-0.5 rounded ${
                      className || ""
                    }`}
                    {...props}
                  >
                    {children}
                  </code>
                );
              },
            }}
          />
        </div>
      ))}
    </div>
  );
}
