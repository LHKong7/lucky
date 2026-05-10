import { useState, useEffect, useCallback, useRef } from "react";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
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
  ollama: "gemma4:e4b",
};

const defaultBaseUrls: Record<string, string> = {
  ollama: "http://localhost:11434/v1",
};

type Panel = "none" | "chat" | "settings";

interface ChatResponse {
  type: "reply" | "approval_needed";
  text?: string;
  tool_call_id?: string;
  tool_name?: string;
  arguments?: any;
  display?: string;
  conversation?: any[];
}

interface PendingApproval {
  tool_call_id: string;
  tool_name: string;
  arguments: any;
  display: string;
  conversation: any[];
}

const BUBBLE_DURATION = 10_000;

function App() {
  const [panel, setPanel] = useState<Panel>("none");
  const [chatInput, setChatInput] = useState("");
  const [bubble, setBubble] = useState<string | null>(null);
  const [bubbleFading, setBubbleFading] = useState(false);
  const [thinking, setThinking] = useState(false);
  const [settings, setSettings] = useState<AppSettings>(defaultSettings);
  const [showKey, setShowKey] = useState(false);
  const [saving, setSaving] = useState(false);
  const [pendingApproval, setPendingApproval] = useState<PendingApproval | null>(null);
  const fadeTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const hideTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  // Keep full history for context but don't display it
  const historyRef = useRef<{ role: string; text: string }[]>([]);

  const isLlmConfigured = (s: AppSettings) => {
    const p = s.llm.provider || "openai";
    if (p === "ollama") return true;
    return !!s.llm.api_key;
  };

  const showBubble = useCallback((text: string) => {
    // Clear existing timers
    if (fadeTimer.current) clearTimeout(fadeTimer.current);
    if (hideTimer.current) clearTimeout(hideTimer.current);
    // Show bubble immediately (reset fade state)
    setBubbleFading(false);
    setBubble(text);
    // Start fade after BUBBLE_DURATION
    fadeTimer.current = setTimeout(() => {
      setBubbleFading(true);
      // Remove bubble after fade animation completes (1s)
      hideTimer.current = setTimeout(() => {
        setBubble(null);
        setBubbleFading(false);
      }, 1000);
    }, BUBBLE_DURATION);
  }, []);

  // Cleanup timers
  useEffect(() => {
    return () => {
      if (fadeTimer.current) clearTimeout(fadeTimer.current);
      if (hideTimer.current) clearTimeout(hideTimer.current);
    };
  }, []);

  // Load settings and session on mount
  useEffect(() => {
    invoke<AppSettings>("load_settings").then((s) => {
      const merged = { ...defaultSettings, ...s, llm: { ...defaultSettings.llm, ...s.llm } };
      setSettings(merged);
      if (!isLlmConfigured(merged)) {
        setPanel("settings");
      }
    });
    // Restore chat history from disk
    invoke<{ role: string; text: string }[]>("load_session").then((msgs) => {
      if (msgs && msgs.length > 0) {
        historyRef.current = msgs;
      }
    }).catch(() => {});
  }, []);

  // Listen for menu bar "Settings" click
  useEffect(() => {
    const unlisten = listen("menu-settings", () => {
      setPanel((prev) => (prev === "settings" ? "none" : "settings"));
    });
    return () => { unlisten.then((f) => f()); };
  }, []);

  // Resize window when panel opens/closes
  useEffect(() => {
    const win = getCurrentWindow();
    if (panel === "none") {
      win.setSize(new LogicalSize(140, 160));
    } else if (panel === "chat") {
      win.setSize(new LogicalSize(280, 240));
    } else if (panel === "settings") {
      win.setSize(new LogicalSize(300, 480));
    }
  }, [panel]);

  // Drag
  const handleDrag = useCallback((e: React.MouseEvent) => {
    if (e.button !== 0) return;
    if ((e.target as HTMLElement).closest("button")) return;
    if ((e.target as HTMLElement).closest("input")) return;
    if ((e.target as HTMLElement).closest("select")) return;
    if ((e.target as HTMLElement).closest(".speech-bubble")) return;
    if ((e.target as HTMLElement).closest(".approval-card")) return;
    e.preventDefault();
    getCurrentWindow().startDragging();
  }, []);

  const togglePanel = (target: Panel) => {
    setPanel((prev) => (prev === target ? "none" : target));
  };

  const handleChatResponse = (resp: ChatResponse) => {
    if (resp.type === "reply") {
      historyRef.current = [...historyRef.current, { role: "pet", text: resp.text! }];
      showBubble(resp.text!);
      invoke("save_session", { messages: historyRef.current }).catch(() => {});
      setPendingApproval(null);
    } else if (resp.type === "approval_needed") {
      setPendingApproval({
        tool_call_id: resp.tool_call_id!,
        tool_name: resp.tool_name!,
        arguments: resp.arguments,
        display: resp.display!,
        conversation: resp.conversation!,
      });
    }
    setThinking(false);
  };

  const handleSendChat = async () => {
    if (!isLlmConfigured(settings)) {
      setPanel("settings");
      return;
    }
    const text = chatInput.trim();
    if (!text) return;
    setChatInput("");
    setThinking(true);
    setPendingApproval(null);

    historyRef.current = [...historyRef.current, { role: "user", text }];

    try {
      const resp = await invoke<ChatResponse>("chat_message", { messages: historyRef.current });
      handleChatResponse(resp);
    } catch (e) {
      showBubble(`Error: ${e}`);
      setThinking(false);
    }
  };

  const handleApproval = async (approved: boolean) => {
    if (!pendingApproval) return;
    setThinking(true);
    setPendingApproval(null);

    try {
      const resp = await invoke<ChatResponse>("continue_chat", {
        conversation: pendingApproval.conversation,
        approved,
        toolCallId: pendingApproval.tool_call_id,
        toolName: pendingApproval.tool_name,
        arguments: pendingApproval.arguments,
      });
      handleChatResponse(resp);
    } catch (e) {
      showBubble(`Error: ${e}`);
      setThinking(false);
    }
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

  const handleProviderChange = (newProvider: string) => {
    setSettings((s) => ({
      ...s,
      llm: {
        ...s.llm,
        provider: newProvider,
        base_url: defaultBaseUrls[newProvider] || null,
      },
    }));
  };

  const provider = settings.llm.provider || "openai";

  return (
    <div className="app">
      {/* Panda + bubble area */}
      <div className={`panda-area ${panel === "chat" ? "chat-open" : ""}`} onMouseDown={handleDrag}>
        {/* Speech bubble — positioned above-right of panda */}
        {(bubble || thinking) && panel === "chat" && !pendingApproval && (
          <div className={`speech-bubble ${bubbleFading ? "fading" : ""}`}>
            <div className="speech-bubble-text">
              {thinking ? "..." : bubble}
            </div>
            <div className="speech-bubble-tail" />
          </div>
        )}

        {/* Approval card */}
        {pendingApproval && panel === "chat" && (
          <div className="approval-card">
            <div className="approval-header">Lucky wants to run:</div>
            <div className="approval-command">{pendingApproval.display}</div>
            <div className="approval-actions">
              <button className="approval-btn reject" onClick={() => handleApproval(false)}>
                Reject
              </button>
              <button className="approval-btn approve" onClick={() => handleApproval(true)}>
                Approve
              </button>
            </div>
          </div>
        )}

        <div className="panda-sprite">
          <div className="pixel" />
        </div>

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
        </div>
      </div>

      {/* Chat input */}
      {panel === "chat" && (
        <div className="chat-input-row">
          <input
            type="text"
            className="chat-input"
            placeholder="Say something..."
            value={chatInput}
            onChange={(e) => setChatInput(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && handleSendChat()}
            autoFocus
          />
          <button className="chat-send" onClick={handleSendChat} disabled={thinking}>
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <line x1="22" y1="2" x2="11" y2="13" />
              <polygon points="22 2 15 22 11 13 2 9 22 2" />
            </svg>
          </button>
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
                onChange={(e) => handleProviderChange(e.target.value)}
              >
                <option value="openai">OpenAI</option>
                <option value="anthropic">Anthropic</option>
                <option value="google">Google</option>
                <option value="ollama">Ollama (Local)</option>
              </select>
            </div>

            <div className="s-group">
              <label className="s-label">API Key {provider === "ollama" && <span className="s-opt">(optional)</span>}</label>
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
