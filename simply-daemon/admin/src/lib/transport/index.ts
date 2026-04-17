// Pluggable transport layer — same frontend code works over HTTP or Tauri IPC.

export type Unsubscribe = () => void;

// ---------------------------------------------------------------------------
// Base URL configuration
// ---------------------------------------------------------------------------

/**
 * Daemon base URL. In production (served by daemon), this is empty (same origin).
 * In dev (Astro on 4321, daemon on 9800), set to 'http://localhost:9800'.
 *
 * Auto-detected: if the page is NOT served by the daemon (port != 9800),
 * assume dev mode and point to localhost:9800.
 */
function detectBaseUrl(): string {
  if (typeof location === 'undefined') return '';
  // If served by the daemon, use same origin (relative URLs)
  if (location.port === '9800') return '';
  // Dev mode: Astro dev server, proxy to daemon
  return 'http://localhost:9800';
}

let _baseUrl: string | null = null;

export function getBaseUrl(): string {
  if (_baseUrl === null) _baseUrl = detectBaseUrl();
  return _baseUrl;
}

export function setBaseUrl(url: string) {
  _baseUrl = url;
}

// ---------------------------------------------------------------------------
// Transport interface
// ---------------------------------------------------------------------------

export interface Transport {
  rpc<T>(method: string, httpMethod: string, path: string, body?: unknown): Promise<T>;
  wsCall<T>(method: string, params?: unknown): Promise<T>;
  subscribe<T>(event: string, callback: (payload: T) => void): Unsubscribe;
  destroy(): void;
}

// ---------------------------------------------------------------------------
// Singleton
// ---------------------------------------------------------------------------

let _instance: Transport | null = null;

export function getTransport(): Transport {
  if (!_instance) _instance = createTransport();
  return _instance;
}

export function createTransport(): Transport {
  if (typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window) {
    throw new Error('TauriTransport not yet implemented — use HttpTransport');
  }
  return new HttpTransportImpl();
}

// ---------------------------------------------------------------------------
// User context
// ---------------------------------------------------------------------------

const USER_KEY = 'simply-admin-user-id';

export function setCurrentUser(userId: string | null) {
  if (userId) localStorage.setItem(USER_KEY, userId);
  else localStorage.removeItem(USER_KEY);
}

export function getCurrentUser(): string | null {
  return localStorage.getItem(USER_KEY);
}

function contextHeaders(): Record<string, string> {
  const userId = getCurrentUser();
  if (!userId) return {};
  return {
    'X-Request-Context': JSON.stringify({ scope: { user_id: userId } }),
  };
}

// ---------------------------------------------------------------------------
// HttpTransport
// ---------------------------------------------------------------------------

class HttpTransportImpl implements Transport {
  private ws: WebSocket | null = null;
  private listeners = new Map<string, Set<(payload: any) => void>>();
  private wsReady: Promise<void> | null = null;
  private wsResolve: (() => void) | null = null;

  async rpc<T>(
    _method: string,
    httpMethod: string,
    path: string,
    body?: unknown,
  ): Promise<T> {
    const url = getBaseUrl() + path;
    const init: RequestInit = {
      method: httpMethod,
      headers: { ...contextHeaders() },
    };
    // Only send body for methods that support it
    if (body !== undefined && !['GET', 'HEAD', 'DELETE'].includes(httpMethod)) {
      (init.headers as Record<string, string>)['Content-Type'] = 'application/json';
      init.body = JSON.stringify(body);
    }
    const res = await fetch(url, init);
    if (!res.ok) {
      const text = await res.text();
      throw new Error(`${res.status}: ${text}`);
    }
    const text = await res.text();
    if (!text) return undefined as T;
    return JSON.parse(text) as T;
  }

  async wsCall<T>(method: string, params: unknown = {}): Promise<T> {
    this.ensureWs();
    await this.wsReady;
    if (!this.ws) throw new Error('WebSocket not connected');

    const id = Math.floor(Math.random() * 1_000_000);
    return new Promise((resolve, reject) => {
      const handler = (ev: MessageEvent) => {
        try {
          const msg = JSON.parse(ev.data);
          if (msg.id === id) {
            this.ws?.removeEventListener('message', handler);
            if (msg.error) reject(new Error(msg.error.message));
            else resolve(msg.result);
          }
        } catch { /* ignore */ }
      };
      this.ws!.addEventListener('message', handler);
      this.ws!.send(JSON.stringify({ id, method, params }));
    });
  }

  subscribe<T>(event: string, callback: (payload: T) => void): Unsubscribe {
    this.ensureWs();
    let cbs = this.listeners.get(event);
    if (!cbs) {
      cbs = new Set();
      this.listeners.set(event, cbs);
    }
    cbs.add(callback);
    return () => {
      cbs!.delete(callback);
      if (cbs!.size === 0) this.listeners.delete(event);
    };
  }

  destroy(): void {
    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }
    this.listeners.clear();
  }

  // -- WebSocket management --

  private ensureWs(): void {
    if (this.ws) return;

    this.wsReady = new Promise((resolve) => { this.wsResolve = resolve; });

    const base = getBaseUrl() || location.origin;
    const wsBase = base.replace(/^http/, 'ws');
    this.ws = new WebSocket(`${wsBase}/ws`);

    this.ws.onopen = () => {
      this.ws!.send(JSON.stringify({
        id: 1,
        method: 'client.identify',
        params: { name: 'admin-ui' },
      }));
      this.wsResolve?.();
    };

    this.ws.onmessage = (ev) => {
      try {
        const msg = JSON.parse(ev.data);
        console.log('[ws] message:', msg);
        if (msg.method && msg.id === undefined) {
          this.dispatch(msg.method, msg.params);
        }
      } catch { /* ignore */ }
    };

    this.ws.onclose = () => {
      this.ws = null;
      this.wsReady = null;
      if (this.listeners.size > 0) {
        setTimeout(() => this.ensureWs(), 2000);
      }
    };

    this.ws.onerror = () => {};
  }

  private dispatch(method: string, params: unknown): void {
    const cbs = this.listeners.get(method);
    if (cbs) {
      console.log(`[ws] dispatch "${method}" → ${cbs.size} listener(s)`);
      for (const cb of cbs) cb(params);
    } else {
      console.log(`[ws] dispatch "${method}" → no listeners (registered: ${[...this.listeners.keys()].join(', ') || 'none'})`);
    }
  }
}

export { HttpTransportImpl as HttpTransport };
