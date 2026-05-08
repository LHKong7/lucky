import { useState, useEffect, useCallback } from "react";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

interface LlmSettings {
  provider: string | null;
  api_key: string | null;
  model: string | null;
  base_url: string | null;
}

interface AppSettings {
  working_directory: string | null;
  llm: LlmSettings;
}

const defaultSettings: AppSettings = {
  working_directory: null,
  llm: { provider: "openai", api_key: null, model: null, base_url: null },
};

const modelPlaceholders: Record<string, string> = {
  openai: "gpt-4o",
  anthropic: "claude-sonnet-4-20250514",
  google: "gemini-2.0-flash",
};

type Panel = "none" | "chat" | "settings";

function App() {
  const [panel, setPanel] = useState<Panel>("none");
  const [chatInput, setChatInput] = useState("");
  const [messages, setMessages] = useState<{ role: string; text: string }[]>([]);
  const [settings, setSettings] = useState<AppSettings>(defaultSettings);
  const [showKey, setShowKey] = useState(false);
  const [saving, setSaving] = useState(false);

  // Load settings on mount
  useEffect(() => {
    invoke<AppSettings>("load_settings").then((s) => {
      setSettings({ ...defaultSettings, ...s, llm: { ...defaultSettings.llm, ...s.llm } });
    });
  }, []);

  // Resize window when panel opens/closes
  useEffect(() => {
    const win = getCurrentWindow();
    if (panel === "none") {
      win.setSize(new LogicalSize(140, 160));
    } else if (panel === "chat") {
      win.setSize(new LogicalSize(300, 420));
    } else if (panel === "settings") {
      win.setSize(new LogicalSize(300, 480));
    }
  }, [panel]);

  // Drag the window by mousedown on panda area
  const handleDrag = useCallback((e: React.MouseEvent) => {
    if (e.button !== 0) return;
    if ((e.target as HTMLElement).closest("button")) return;
    if ((e.target as HTMLElement).closest("input")) return;
    if ((e.target as HTMLElement).closest("select")) return;
    e.preventDefault();
    getCurrentWindow().startDragging();
  }, []);

  const togglePanel = (target: Panel) => {
    setPanel((prev) => (prev === target ? "none" : target));
  };

  const handleSendChat = () => {
    const text = chatInput.trim();
    if (!text) return;
    setMessages((m) => [...m, { role: "user", text }]);
    setChatInput("");
    // TODO: wire to agent backend
    setTimeout(() => {
      setMessages((m) => [...m, { role: "pet", text: "Munch munch~ I'm still learning to chat! Configure my AI settings first." }]);
    }, 600);
  };

  const handleSaveSettings = async () => {
    setSaving(true);
    try {
      await invoke("save_settings", { settings });
      setSaving(false);
      setPanel("none");
    } catch (e) {
      console.error("Failed to save:", e);
      setSaving(false);
    }
  };

  const handlePickDir = async () => {
    const dir = await invoke<string | null>("pick_directory");
    if (dir) setSettings((s) => ({ ...s, working_directory: dir }));
  };

  const updateLlm = (field: keyof LlmSettings, value: string) => {
    setSettings((s) => ({ ...s, llm: { ...s.llm, [field]: value || null } }));
  };

  const provider = settings.llm.provider || "openai";

  return (
    <div className="app">
      {/* Panda area — draggable */}
      <div className="panda-area" onMouseDown={handleDrag}>
        <div className="panda-sprite">
          <div className="pixel" />
        </div>

        {/* Toolbar — bottom of panda area */}
        <div className="toolbar">
          <button
            className={`toolbar-btn ${panel === "chat" ? "active" : ""}`}
            onClick={() => togglePanel("chat")}
            title="Chat"
          >
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
            </svg>
          </button>
          <button
            className={`toolbar-btn ${panel === "settings" ? "active" : ""}`}
            onClick={() => togglePanel("settings")}
            title="Settings"
          >
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <circle cx="12" cy="12" r="3" />
              <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
            </svg>
          </button>
        </div>
      </div>

      {/* Chat panel */}
      {panel === "chat" && (
        <div className="panel chat-panel">
          <div className="chat-messages">
            {messages.length === 0 && (
              <div className="chat-empty">Click to chat with your panda~</div>
            )}
            {messages.map((m, i) => (
              <div key={i} className={`chat-msg ${m.role}`}>
                {m.role === "pet" && <span className="chat-avatar">🐼</span>}
                <span className="chat-text">{m.text}</span>
              </div>
            ))}
          </div>
          <div className="chat-input-row">
            <input
              type="text"
              className="chat-input"
              placeholder="Say something..."
              value={chatInput}
              onChange={(e) => setChatInput(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && handleSendChat()}
            />
            <button className="chat-send" onClick={handleSendChat}>
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <line x1="22" y1="2" x2="11" y2="13" />
                <polygon points="22 2 15 22 11 13 2 9 22 2" />
              </svg>
            </button>
          </div>
        </div>
      )}

      {/* Settings panel */}
      {panel === "settings" && (
        <div className="panel settings-panel">
          <div className="settings-scroll">
            <div className="s-group">
              <label className="s-label">Working Directory</label>
              <div className="s-row">
                <input
                  type="text"
                  className="s-input"
                  value={settings.working_directory || ""}
                  onChange={(e) => setSettings((s) => ({ ...s, working_directory: e.target.value || null }))}
                  placeholder="Select folder..."
                />
                <button className="s-icon-btn" onClick={handlePickDir} title="Browse">
                  <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                    <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
                  </svg>
                </button>
              </div>
            </div>

            <div className="s-group">
              <label className="s-label">Provider</label>
              <select
                className="s-input"
                value={provider}
                onChange={(e) => updateLlm("provider", e.target.value)}
              >
                <option value="openai">OpenAI</option>
                <option value="anthropic">Anthropic</option>
                <option value="google">Google</option>
              </select>
            </div>

            <div className="s-group">
              <label className="s-label">API Key</label>
              <div className="s-row">
                <input
                  type={showKey ? "text" : "password"}
                  className="s-input"
                  value={settings.llm.api_key || ""}
                  onChange={(e) => updateLlm("api_key", e.target.value)}
                  placeholder="Enter API key"
                />
                <button className="s-icon-btn" onClick={() => setShowKey((v) => !v)}>
                  {showKey ? "Hide" : "Show"}
                </button>
              </div>
            </div>

            <div className="s-group">
              <label className="s-label">Model <span className="s-opt">(optional)</span></label>
              <input
                type="text"
                className="s-input"
                value={settings.llm.model || ""}
                onChange={(e) => updateLlm("model", e.target.value)}
                placeholder={modelPlaceholders[provider] || ""}
              />
            </div>

            <div className="s-group">
              <label className="s-label">Base URL <span className="s-opt">(optional)</span></label>
              <input
                type="text"
                className="s-input"
                value={settings.llm.base_url || ""}
                onChange={(e) => updateLlm("base_url", e.target.value)}
                placeholder="Custom endpoint"
              />
            </div>
          </div>
          <div className="s-actions">
            <button className="s-btn-cancel" onClick={() => setPanel("none")}>Cancel</button>
            <button className="s-btn-save" onClick={handleSaveSettings} disabled={saving}>
              {saving ? "..." : "Save"}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

export default App;
