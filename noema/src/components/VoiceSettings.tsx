import { useState, useEffect, useCallback } from "react";
import * as tauri from "../tauri";

interface VoiceSettingsProps {
  sttProvider: string | null;
  ttsProvider: string | null;
  ttsVoice: string | null;
  onSttProviderChange: (id: string) => void;
  onTtsProviderChange: (id: string) => void;
  onTtsVoiceChange: (id: string) => void;
}

export function VoiceSettings({
  sttProvider,
  ttsProvider,
  ttsVoice,
  onSttProviderChange,
  onTtsProviderChange,
  onTtsVoiceChange,
}: VoiceSettingsProps) {
  const [providers, setProviders] = useState<tauri.VoiceProviderInfo[]>([]);
  const [voices, setVoices] = useState<{ id: string; name: string }[]>([]);
  const [loadingVoices, setLoadingVoices] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Load providers
  useEffect(() => {
    tauri.listVoiceProviders()
      .then(setProviders)
      .catch((e) => setError(String(e)));
  }, []);

  // Load voices when TTS provider changes
  const loadVoices = useCallback(async (providerId: string) => {
    if (!providerId) return;
    setLoadingVoices(true);
    setError(null);
    try {
      const result = await tauri.listTtsVoices(providerId);
      setVoices(result);
    } catch (e) {
      setVoices([]);
      // Not all providers support voice listing
    } finally {
      setLoadingVoices(false);
    }
  }, []);

  useEffect(() => {
    if (ttsProvider) {
      loadVoices(ttsProvider);
    }
  }, [ttsProvider, loadVoices]);

  const sttProviders = providers.filter((p) => p.capabilities.includes("stt"));
  const ttsProviders = providers.filter((p) => p.capabilities.includes("tts"));

  return (
    <div className="space-y-6">
      {error && (
        <div className="text-red-400 text-sm bg-red-900/20 rounded p-3">{error}</div>
      )}

      {/* STT Provider */}
      <div>
        <label className="block text-sm font-medium text-foreground mb-2">
          Speech-to-Text Provider
        </label>
        <p className="text-xs text-muted mb-2">
          Used for transcribing your speech into text.
        </p>
        <select
          value={sttProvider ?? ""}
          onChange={(e) => onSttProviderChange(e.target.value)}
          className="w-full h-10 px-3 bg-elevated border border-gray-600 rounded-lg text-foreground focus:outline-none focus:ring-1 focus:ring-teal-500"
        >
          <option value="" disabled>Select STT provider</option>
          {sttProviders.map((p) => (
            <option key={p.id} value={p.id}>{p.name}</option>
          ))}
        </select>
      </div>

      {/* TTS Provider */}
      <div>
        <label className="block text-sm font-medium text-foreground mb-2">
          Text-to-Speech Provider
        </label>
        <p className="text-xs text-muted mb-2">
          Used for speaking assistant responses aloud.
        </p>
        <select
          value={ttsProvider ?? ""}
          onChange={(e) => onTtsProviderChange(e.target.value)}
          className="w-full h-10 px-3 bg-elevated border border-gray-600 rounded-lg text-foreground focus:outline-none focus:ring-1 focus:ring-teal-500"
        >
          <option value="">None (text only)</option>
          {ttsProviders.map((p) => (
            <option key={p.id} value={p.id}>{p.name}</option>
          ))}
        </select>
      </div>

      {/* Voice Selection */}
      {ttsProvider && (
        <div>
          <label className="block text-sm font-medium text-foreground mb-2">
            Voice
          </label>
          <p className="text-xs text-muted mb-2">
            Select a voice for speech output.
          </p>
          {loadingVoices ? (
            <div className="text-muted text-sm">Loading voices...</div>
          ) : voices.length > 0 ? (
            <select
              value={ttsVoice ?? ""}
              onChange={(e) => onTtsVoiceChange(e.target.value)}
              className="w-full h-10 px-3 bg-elevated border border-gray-600 rounded-lg text-foreground focus:outline-none focus:ring-1 focus:ring-teal-500"
            >
              <option value="">Default</option>
              {voices.map((v) => (
                <option key={v.id} value={v.id}>{v.name}</option>
              ))}
            </select>
          ) : (
            <div className="text-muted text-sm">No voices available for this provider.</div>
          )}
        </div>
      )}
    </div>
  );
}
