import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./HistoryApp.css";

interface Message {
  role: string;
  text: string;
}

function HistoryApp() {
  const [messages, setMessages] = useState<Message[]>([]);
  const [loading, setLoading] = useState(true);
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    invoke<Message[]>("load_session")
      .then((msgs) => {
        setMessages(msgs || []);
        setLoading(false);
      })
      .catch(() => setLoading(false));
  }, []);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  const handleClear = async () => {
    if (!confirm("Clear all chat history?")) return;
    await invoke("save_session", { messages: [] }).catch(() => {});
    setMessages([]);
  };

  if (loading) {
    return <div className="history-loading">Loading...</div>;
  }

  return (
    <div className="history-root">
      <div className="history-header">
        <h1>Chat History</h1>
        <button className="history-clear" onClick={handleClear} disabled={messages.length === 0}>
          Clear
        </button>
      </div>
      <div className="history-messages">
        {messages.length === 0 && (
          <div className="history-empty">No messages yet.</div>
        )}
        {messages.map((m, i) => (
          <div key={i} className={`history-msg ${m.role}`}>
            <div className="history-msg-role">
              {m.role === "pet" ? "Lucky" : "You"}
            </div>
            <div className="history-msg-text">{m.text}</div>
          </div>
        ))}
        <div ref={bottomRef} />
      </div>
    </div>
  );
}

export default HistoryApp;
