<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { getTransport, getBaseUrl, voiceApi, type VoiceProviderInfo } from '@simply/client';

  interface Props {
    onTranscription: (text: string) => void;
    autoSend?: boolean;
  }
  let { onTranscription, autoSend = true }: Props = $props();

  const t = getTransport();
  const voice = voiceApi(t);

  let providers = $state<VoiceProviderInfo[]>([]);
  let sttProvider = $state<string | null>(null);
  let status = $state<'disabled' | 'listening' | 'transcribing' | 'idle'>('disabled');
  let error = $state<string | null>(null);
  let chunksSent = $state(0);

  // Audio capture
  let audioContext: AudioContext | null = null;
  let workletNode: AudioWorkletNode | null = null;
  let mediaStream: MediaStream | null = null;

  // Voice WebSocket
  let voiceWs: WebSocket | null = null;

  onMount(async () => {
    try {
      providers = await voice.listVoiceProviders();
      console.log('[voice] providers:', providers);
      const stt = providers.find(p => p.capabilities.includes('stt'));
      sttProvider = stt?.id ?? null;
      console.log('[voice] selected STT provider:', sttProvider);
      if (sttProvider) status = 'idle';
    } catch (e) {
      console.error('[voice] load providers failed:', e);
    }
  });

  onDestroy(() => stopRecording());

  async function toggle() {
    console.log('[voice] toggle, current status:', status);
    if (status === 'listening') {
      await stopRecording();
    } else if (status === 'idle' || status === 'disabled') {
      await startRecording();
    }
  }

  function handleVoiceEvent(event: unknown) {
    if (typeof event === 'string') {
      console.log('[voice] event:', event);
      if (event === 'Listening') status = 'listening';
      else if (event === 'Transcribing') status = 'transcribing';
      else if (event === 'TurnEnd') status = 'idle';
    } else if (typeof event === 'object' && event !== null) {
      const e = event as Record<string, unknown>;
      if ('UserTranscript' in e) {
        console.log('[voice] transcript:', e.UserTranscript);
        status = 'idle';
        onTranscription(e.UserTranscript as string);
      } else if ('ModelTranscript' in e) {
        console.log('[voice] model transcript:', e.ModelTranscript);
      } else if ('Error' in e) {
        console.error('[voice] stream error:', e.Error);
        error = e.Error as string;
        stopRecording();
      } else if ('Audio' in e) {
        console.log('[voice] audio output received');
      } else {
        console.warn('[voice] unknown event:', event);
      }
    }
  }

  async function startRecording() {
    if (!sttProvider) { error = 'No STT provider available'; return; }
    error = null;
    chunksSent = 0;

    try {
      if (!navigator.mediaDevices?.getUserMedia) {
        throw new Error('Microphone access requires HTTPS');
      }

      // Open voice WebSocket
      const base = getBaseUrl() || location.origin;
      const wsBase = base.replace(/^http/, 'ws');
      const wsUrl = `${wsBase}/api/voice/stream/${sttProvider}`;
      console.log('[voice] opening WebSocket:', wsUrl);
      voiceWs = new WebSocket(wsUrl);

      voiceWs.onmessage = (ev) => {
        try {
          const raw = ev.data;
          console.log('[voice] WS message:', raw);
          const msg = JSON.parse(raw);

          // Daemon wraps stream events as WsNotification: { method, params }
          // The actual VoiceEvent is in params
          const event = (msg.method && msg.params !== undefined) ? msg.params : msg;

          // Skip WsResponse messages (e.g. {id: 0, result: true})
          if (msg.id !== undefined && msg.result !== undefined) {
            console.log('[voice] WS response (init):', msg);
            return;
          }

          handleVoiceEvent(event);
        } catch (e) {
          console.error('[voice] failed to parse WS message:', ev.data, e);
        }
      };

      voiceWs.onerror = (ev) => {
        console.error('[voice] WebSocket error:', ev);
        error = 'Voice WebSocket error';
        stopRecording();
      };

      voiceWs.onclose = (ev) => {
        console.log('[voice] WebSocket closed, code:', ev.code, 'reason:', ev.reason);
        voiceWs = null;
      };

      // Wait for WS to open
      await new Promise<void>((resolve, reject) => {
        voiceWs!.onopen = () => {
          console.log('[voice] WebSocket connected');
          resolve();
        };
        setTimeout(() => reject(new Error('Voice WS timeout')), 5000);
      });

      // Capture audio
      console.log('[voice] requesting microphone access');
      mediaStream = await navigator.mediaDevices.getUserMedia({
        audio: { channelCount: 1, sampleRate: 16000, echoCancellation: true, noiseSuppression: true },
      });
      console.log('[voice] microphone access granted, tracks:', mediaStream.getAudioTracks().length);

      audioContext = new AudioContext({ sampleRate: 16000 });
      console.log('[voice] AudioContext created, sampleRate:', audioContext.sampleRate);

      await audioContext.audioWorklet.addModule('/audio-processor.js');
      console.log('[voice] AudioWorklet loaded');

      const source = audioContext.createMediaStreamSource(mediaStream);
      workletNode = new AudioWorkletNode(audioContext, 'audio-capture-processor');

      workletNode.port.onmessage = (event) => {
        if (event.data.type === 'audio' && voiceWs?.readyState === WebSocket.OPEN) {
          const f32Samples = event.data.samples as Float32Array;
          // Convert f32 samples to PCM16 LE bytes (what AudioChunk.data expects)
          const pcm16 = new Int16Array(f32Samples.length);
          for (let i = 0; i < f32Samples.length; i++) {
            const s = Math.max(-1, Math.min(1, f32Samples[i]));
            pcm16[i] = s < 0 ? s * 0x8000 : s * 0x7FFF;
          }
          // Serialize as byte array (Vec<u8> in Rust)
          const bytes = Array.from(new Uint8Array(pcm16.buffer));
          const msg = JSON.stringify({ Audio: { data: bytes } });
          voiceWs.send(msg);
          chunksSent++;
          if (chunksSent <= 3 || chunksSent % 20 === 0) {
            console.log(`[voice] sent chunk #${chunksSent}, ${f32Samples.length} samples → ${bytes.length} bytes`);
          }
        }
      };

      source.connect(workletNode);
      const gain = audioContext.createGain();
      gain.gain.value = 0;
      workletNode.connect(gain);
      gain.connect(audioContext.destination);

      status = 'listening';
      console.log('[voice] recording started, status:', status);
    } catch (e) {
      console.error('[voice] start failed:', e);
      error = `${e}`;
      status = sttProvider ? 'idle' : 'disabled';
      stopRecording();
    }
  }

  async function stopRecording() {
    console.log('[voice] stopping, chunks sent:', chunksSent);
    if (workletNode) { workletNode.disconnect(); workletNode = null; }
    if (audioContext) { await audioContext.close().catch(() => {}); audioContext = null; }
    if (mediaStream) { mediaStream.getTracks().forEach(t => t.stop()); mediaStream = null; }
    if (voiceWs) { voiceWs.close(); voiceWs = null; }
    if (status === 'listening') status = sttProvider ? 'idle' : 'disabled';
    console.log('[voice] stopped, status:', status);
  }

  const statusLabel = $derived(
    status === 'listening' ? `🔴 ${chunksSent}` :
    status === 'transcribing' ? '⏳' :
    status === 'idle' ? '🎤' :
    '🎤'
  );

  const buttonDisabled = $derived(status === 'transcribing' || status === 'disabled');
</script>

<button
  class="px-3 py-2 rounded-lg text-sm shrink-0
         {status === 'listening' ? 'bg-red-500/20 text-red-400 animate-pulse' :
          status === 'transcribing' ? 'bg-yellow-500/20 text-yellow-400' :
          'bg-white/5 text-muted hover:text-fg'}
         disabled:opacity-30 disabled:cursor-not-allowed"
  disabled={buttonDisabled}
  onclick={toggle}
  title={error || `Status: ${status}, provider: ${sttProvider || 'none'}, chunks: ${chunksSent}`}
>
  {statusLabel}
</button>

{#if error}
  <span class="text-xs text-red-400">{error}</span>
{/if}
