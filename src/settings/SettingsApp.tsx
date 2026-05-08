import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import "./SettingsApp.css";

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
  llm: {
    provider: "openai",
    api_key: null,
    model: null,
    base_url: null,
  },
};

const modelPlaceholders: Record<string, string> = {
  openai: "gpt-4o",
  anthropic: "claude-sonnet-4-20250514",
  google: "gemini-2.0-flash",
};

function SettingsApp() {
  const [settings, setSettings] = useState<AppSettings>(defaultSettings);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [showKey, setShowKey] = useState(false);

  useEffect(() => {
    invoke<AppSettings>("load_settings").then((s) => {
      setSettings({ ...defaultSettings, ...s, llm: { ...defaultSettings.llm, ...s.llm } });
      setLoading(false);
    });
  }, []);

  const handleSave = async () => {
    setSaving(true);
    try {
      await invoke("save_settings", { settings });
      getCurrentWebviewWindow().close();
    } catch (e) {
      console.error("Failed to save settings:", e);
      setSaving(false);
    }
  };

  const handleCancel = () => {
    getCurrentWebviewWindow().close();
  };

  const handlePickDirectory = async () => {
    const dir = await invoke<string | null>("pick_directory");
    if (dir) {
      setSettings((s) => ({ ...s, working_directory: dir }));
    }
  };

  const updateLlm = (field: keyof LlmSettings, value: string) => {
    setSettings((s) => ({
      ...s,
      llm: { ...s.llm, [field]: value || null },
    }));
  };

  if (loading) {
    return <div className="settings-loading">Loading...</div>;
  }

  const provider = settings.llm.provider || "openai";

  return (
    <div className="settings-root">
      <h1 className="settings-title">Settings</h1>

      <section className="settings-section">
        <h2>Working Directory</h2>
        <p className="settings-hint">Where agent data, sessions, and skills are stored.</p>
        <div className="input-row">
          <input
            type="text"
            value={settings.working_directory || ""}
            onChange={(e) => setSettings((s) => ({ ...s, working_directory: e.target.value || null }))}
            placeholder="Select a directory..."
          />
          <button className="btn-icon" onClick={handlePickDirectory} title="Browse">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
            </svg>
          </button>
        </div>
      </section>

      <section className="settings-section">
        <h2>AI Provider</h2>

        <label className="settings-label">Provider</label>
        <select
          value={provider}
          onChange={(e) => updateLlm("provider", e.target.value)}
        >
          <option value="openai">OpenAI</option>
          <option value="anthropic">Anthropic</option>
          <option value="google">Google</option>
        </select>

        <label className="settings-label">API Key</label>
        <div className="input-row">
          <input
            type={showKey ? "text" : "password"}
            value={settings.llm.api_key || ""}
            onChange={(e) => updateLlm("api_key", e.target.value)}
            placeholder="Enter your API key"
          />
          <button
            className="btn-icon"
            onClick={() => setShowKey((v) => !v)}
            title={showKey ? "Hide" : "Show"}
          >
            {showKey ? (
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94" />
                <path d="M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19" />
                <line x1="1" y1="1" x2="23" y2="23" />
              </svg>
            ) : (
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z" />
                <circle cx="12" cy="12" r="3" />
              </svg>
            )}
          </button>
        </div>

        <label className="settings-label">Model <span className="optional">(optional)</span></label>
        <input
          type="text"
          value={settings.llm.model || ""}
          onChange={(e) => updateLlm("model", e.target.value)}
          placeholder={modelPlaceholders[provider] || ""}
        />

        <label className="settings-label">Base URL <span className="optional">(optional)</span></label>
        <input
          type="text"
          value={settings.llm.base_url || ""}
          onChange={(e) => updateLlm("base_url", e.target.value)}
          placeholder="Custom API endpoint"
        />
      </section>

      <div className="settings-actions">
        <button className="btn-secondary" onClick={handleCancel}>Cancel</button>
        <button className="btn-primary" onClick={handleSave} disabled={saving}>
          {saving ? "Saving..." : "Save"}
        </button>
      </div>
    </div>
  );
}

export default SettingsApp;
