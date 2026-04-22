<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { voiceApi, getTransport, type VoiceProviderInfo } from '@simply/client';

  type Props = {
    onTranscription: (text: string) => void;
  };
  const { onTranscription }: Props = $props();

  // Audio capture lives in the Tauri host (see noema/src-tauri/src/voice.rs),
  // so this component only drives it via invoke + listen — no webview audio
  // plumbing.
  const isTauri =
    typeof window !== 'undefined' && !!(window as any).__TAURI_INTERNALS__;

  let providers = $state<VoiceProviderInfo[]>([]);
  let sttProvider = $state<string | null>(null);
  let status = $state<'disabled' | 'idle' | 'listening' | 'transcribing'>('disabled');
  let error = $state<string | null>(null);

  let unlisten: UnlistenFn | null = null;

  onMount(async () => {
    if (isTauri) {
      unlisten = await listen<unknown>('voice_event', (event) => {
        handleVoiceEvent(event.payload);
      });
    }

    try {
      providers = await voiceApi(getTransport()).listVoiceProviders();
      const stt = providers.find((p) => p.capabilities.includes('stt'));
      sttProvider = stt?.id ?? null;
      if (sttProvider && isTauri) status = 'idle';
    } catch (e) {
      console.error('[voice] load providers failed:', e);
    }
  });

  onDestroy(async () => {
    if (unlisten) unlisten();
    await stop();
  });

  function handleVoiceEvent(event: unknown) {
    if (typeof event === 'string') {
      if (event === 'Listening') status = 'listening';
      else if (event === 'Transcribing') status = 'transcribing';
      else if (event === 'TurnEnd') status = 'idle';
      return;
    }
    if (typeof event === 'object' && event !== null) {
      const e = event as Record<string, unknown>;
      if ('UserTranscript' in e) {
        status = 'idle';
        onTranscription(e.UserTranscript as string);
      } else if ('Error' in e) {
        error = String(e.Error);
        status = sttProvider ? 'idle' : 'disabled';
      }
    }
  }

  async function toggle() {
    if (status === 'listening' || status === 'transcribing') {
      await stop();
    } else if (status === 'idle') {
      await start();
    }
  }

  async function start() {
    if (!isTauri) {
      error = 'Voice capture requires the Noema desktop app.';
      return;
    }
    if (!sttProvider) {
      error = 'No STT provider available';
      return;
    }
    error = null;
    try {
      await invoke('start_voice_capture', {
        providerId: sttProvider,
        deviceId: null,
      });
      status = 'listening';
    } catch (e) {
      error = `${e}`;
      status = 'idle';
    }
  }

  async function stop() {
    if (!isTauri) return;
    try {
      await invoke('stop_voice_capture');
    } catch (e) {
      console.warn('[voice] stop failed', e);
    }
    if (status === 'listening' || status === 'transcribing') {
      status = sttProvider ? 'idle' : 'disabled';
    }
  }

  const label = $derived(
    status === 'listening' ? '🔴' : status === 'transcribing' ? '⏳' : '🎤',
  );

  const tooltip = $derived(
    error ??
      (status === 'disabled'
        ? isTauri
          ? 'No STT provider configured'
          : 'Voice capture only available in the desktop app'
        : `Status: ${status}${sttProvider ? ` · ${sttProvider}` : ''}`),
  );

  const buttonDisabled = $derived(status === 'disabled' || status === 'transcribing');
</script>

<button
  class="shrink-0 rounded-lg px-3 py-2 text-sm transition-colors {status === 'listening'
    ? 'animate-pulse bg-red-500/20 text-red-400'
    : status === 'transcribing'
      ? 'bg-yellow-500/20 text-yellow-400'
      : 'bg-elevated text-muted hover:text-foreground'} disabled:cursor-not-allowed disabled:opacity-30"
  disabled={buttonDisabled}
  onclick={toggle}
  title={tooltip}
  aria-label="Voice input"
>
  {label}
</button>

{#if error}
  <span class="text-xs text-red-400">{error}</span>
{/if}
