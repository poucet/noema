// Pluggable transport layer — same frontend code works over HTTP or Tauri IPC.

export type Unsubscribe = () => void;

/**
 * Transport abstraction for daemon communication.
 *
 * Two implementations:
 * - HttpTransport: fetch + WebSocket (browser / admin UI)
 * - TauriTransport: invoke + listen (Tauri desktop app)
 */
export interface Transport {
  /**
   * Call an RPC method.
   * @param method  - RPC method name (e.g. "session.list_sessions")
   * @param httpMethod - HTTP method for the REST fallback ("GET", "POST", etc.)
   * @param path    - REST path (e.g. "/api/session")
   * @param body    - Optional JSON body
   */
  rpc<T>(method: string, httpMethod: string, path: string, body?: unknown): Promise<T>;

  /**
   * Subscribe to server-pushed events.
   * Returns an unsubscribe function.
   */
  subscribe<T>(event: string, callback: (payload: T) => void): Unsubscribe;

  /** Clean up connections (WebSocket, listeners, etc.) */
  destroy(): void;
}

/** Detect environment and create the appropriate transport. */
export function createTransport(): Transport {
  if (typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window) {
    // Lazy import to avoid bundling Tauri deps in web builds
    throw new Error('TauriTransport not yet implemented — use HttpTransport');
  }
  // Inline import to keep this file dependency-free
  return new HttpTransportImpl();
}

// ---------------------------------------------------------------------------
// HttpTransport (inline to avoid circular deps in barrel)
// ---------------------------------------------------------------------------

/** User context for scoped API calls. */
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
    const init: RequestInit = {
      method: httpMethod,
      headers: { ...contextHeaders() },
    };
    if (body !== undefined) {
      (init.headers as Record<string, string>)['Content-Type'] = 'application/json';
      init.body = JSON.stringify(body);
    }
    const res = await fetch(path, init);
    if (!res.ok) {
      const text = await res.text();
      throw new Error(`${res.status}: ${text}`);
    }
    const text = await res.text();
    if (!text) return undefined as T;
    return JSON.parse(text) as T;
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

  // -------------------------------------------------------------------------
  // WebSocket management
  // -------------------------------------------------------------------------

  private ensureWs(): void {
    if (this.ws) return;

    this.wsReady = new Promise((resolve) => { this.wsResolve = resolve; });

    const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
    const url = `${proto}//${location.host}/ws`;
    this.ws = new WebSocket(url);

    this.ws.onopen = () => {
      // Identify ourselves
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
        // WsNotification: { method, params } (no id)
        if (msg.method && msg.id === undefined) {
          this.dispatch(msg.method, msg.params);
        }
      } catch {
        // ignore malformed messages
      }
    };

    this.ws.onclose = () => {
      this.ws = null;
      this.wsReady = null;
      // Auto-reconnect after 2s if there are active listeners
      if (this.listeners.size > 0) {
        setTimeout(() => this.ensureWs(), 2000);
      }
    };

    this.ws.onerror = () => {
      // onclose will fire after onerror
    };
  }

  private dispatch(method: string, params: unknown): void {
    // Route "session.event" notifications by extracting the DaemonEvent
    // The daemon sends: { method: "session.event", params: { session_id, event } }
    const cbs = this.listeners.get(method);
    if (cbs) {
      for (const cb of cbs) cb(params);
    }
  }

  /** Send a WS request and wait for the response (for stream subscriptions). */
  async wsCall(method: string, params: unknown = {}): Promise<unknown> {
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
}

export { HttpTransportImpl as HttpTransport };
