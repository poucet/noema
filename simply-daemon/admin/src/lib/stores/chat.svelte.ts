// Chat store — reactive state for conversation UI.
// Uses .svelte.ts extension so Svelte 5 runes ($state) are available.

import {
  getTransport,
  conversationApi,
  sessionApi,
  modelApi,
  type ConversationInfo,
  type ResolvedMessage,
  type ResolvedContent,
  type SessionInfo,
  type ModelInfo,
  type DaemonEvent,
  type ContentBlock,
  type Transport,
  type Unsubscribe,
} from '@simply/client';

// ---------------------------------------------------------------------------
// Display types (presentation layer over ResolvedMessage)
// ---------------------------------------------------------------------------

export interface DisplayMessage {
  role: 'user' | 'assistant' | 'system';
  content: ResolvedContent[];
  turnId?: string;
}

// ---------------------------------------------------------------------------
// Chat store
// ---------------------------------------------------------------------------

function setError(store: ChatStore, msg: string, e: unknown) {
  const full = `${msg}: ${e}`;
  console.error('[chat]', full, e);
  store.error = full;
}

class ChatStore {
  // --- State ---
  conversations = $state<ConversationInfo[]>([]);
  currentConversationId = $state<string | null>(null);
  currentSessionId = $state<string | null>(null);
  messages = $state<DisplayMessage[]>([]);
  streamingMessage = $state<DisplayMessage | null>(null);
  isLoading = $state(false);
  models = $state<ModelInfo[]>([]);
  currentModelId = $state<string>('');
  error = $state<string | null>(null);

  // --- Derived ---
  get currentConversation(): ConversationInfo | undefined {
    return this.conversations.find(c => c.id === this.currentConversationId);
  }

  // --- Private (lazy-initialized in init()) ---
  private transport!: Transport;
  private conv!: ReturnType<typeof conversationApi>;
  private sess!: ReturnType<typeof sessionApi>;
  private model!: ReturnType<typeof modelApi>;
  private unsubs: Unsubscribe[] = [];

  // --- Init ---

  async init() {
    this.transport = getTransport();
    this.conv = conversationApi(this.transport);
    this.sess = sessionApi(this.transport);
    this.model = modelApi(this.transport);

    await Promise.all([
      this.loadConversations(),
      this.loadModels(),
    ]);
  }

  // --- Conversations ---

  async loadConversations() {
    try {
      this.conversations = await this.conv.listConversations();
    } catch (e) {
      setError(this, 'Failed to load conversations', e);
    }
  }

  async newConversation(name?: string) {
    try {
      const convName = name || `Chat ${new Date().toLocaleString('en-US', { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })}`;
      const convId = await this.conv.createConversation(convName);
      await this.loadConversations();
      await this.selectConversation(convId);
    } catch (e) {
      setError(this, 'Failed to create conversation', e);
    }
  }

  async selectConversation(conversationId: string) {
    // Clean up previous session
    this.unsubscribeEvents();
    if (this.currentSessionId) {
      try { await this.sess.closeSession(this.currentSessionId); } catch {}
    }

    this.currentConversationId = conversationId;
    this.streamingMessage = null;
    this.error = null;

    // Load messages
    try {
      const resolved = await this.conv.getMessages(conversationId);
      this.messages = resolved.map(toDisplayMessage);
    } catch (e) {
      this.messages = [];
      setError(this, 'Failed to load messages', e);
    }

    // Create session via WS (stream endpoint) for event subscription
    try {
      const result = await this.transport.wsCall<SessionInfo>(
        'session.create_session',
        { persistence: { Persistent: { conversation_id: conversationId } } },
      );
      this.currentSessionId = result.id;
      this.currentModelId = result.modelId;
    } catch (e) {
      setError(this, 'Session creation failed', e);
      return;
    }

    // Subscribe to session events on the same WebSocket
    this.subscribeEvents();
  }

  async deleteConversation(conversationId: string) {
    try {
      await this.conv.deleteConversation(conversationId);
      if (this.currentConversationId === conversationId) {
        this.currentConversationId = null;
        this.currentSessionId = null;
        this.messages = [];
        this.streamingMessage = null;
        this.unsubscribeEvents();
      }
      await this.loadConversations();
    } catch (e) {
      setError(this, 'Failed to delete', e);
    }
  }

  async renameConversation(conversationId: string, name: string) {
    try {
      await this.conv.renameConversation(conversationId, name);
      await this.loadConversations();
    } catch (e) {
      setError(this, 'Failed to rename', e);
    }
  }

  // --- Messaging ---

  async sendMessage(text: string) {
    if (!this.currentSessionId || !text.trim()) return;

    console.log('[chat] sendMessage, session:', this.currentSessionId);
    this.isLoading = true;
    this.error = null;
    this.streamingMessage = null;

    // Optimistic: add user message to display
    this.messages = [...this.messages, {
      role: 'user',
      content: [{ type: 'text', text }],
    }];

    try {
      await this.sess.sendMessage(this.currentSessionId, {
        content: [{ type: 'text', text }],
      });
      console.log('[chat] sendMessage succeeded, waiting for events...');
    } catch (e) {
      setError(this, 'Failed to send', e);
      this.isLoading = false;
    }
  }

  // --- Models ---

  async loadModels() {
    try {
      this.models = await this.model.listModels();
      this.currentModelId = await this.model.defaultModelId();
    } catch (e) {
      setError(this, 'Failed to load models', e);
    }
  }

  async setModel(modelId: string) {
    if (!this.currentSessionId) return;
    try {
      await this.sess.setModel(this.currentSessionId, modelId);
      this.currentModelId = modelId;
    } catch (e) {
      setError(this, 'Failed to set model', e);
    }
  }

  // --- Event subscription ---

  private subscribeEvents() {
    // Session stream events are sent as "session.create_session.event" notifications
    const unsub = this.transport.subscribe<DaemonEvent>(
      'session.create_session.event',
      (event) => {
        console.log('[chat] daemon event:', event);
        if (!this.currentConversationId) return;
        this.handleDaemonEvent(event);
      },
    );
    this.unsubs.push(unsub);
  }

  private unsubscribeEvents() {
    for (const unsub of this.unsubs) unsub();
    this.unsubs = [];
  }

  private handleDaemonEvent(event: DaemonEvent) {
    if (typeof event === 'string') {
      if (event === 'TurnComplete') {
        if (this.streamingMessage) {
          this.messages = [...this.messages, this.streamingMessage];
          this.streamingMessage = null;
        }
        this.isLoading = false;
      }
      return;
    }

    if ('AssistantContent' in event) {
      const block = event.AssistantContent as ContentBlock;
      // Skip text blocks — those are already handled incrementally via TextDelta.
      // Only handle non-text content blocks (tool calls, images, etc.)
      if (block.type === 'text') return;
      const content = block as unknown as ResolvedContent;
      if (this.streamingMessage) {
        this.streamingMessage = {
          ...this.streamingMessage,
          content: [...this.streamingMessage.content, content],
        };
      } else {
        this.streamingMessage = { role: 'assistant', content: [content] };
      }
    } else if ('TextDelta' in event) {
      const delta = event.TextDelta as string;
      if (this.streamingMessage) {
        const lastContent = this.streamingMessage.content;
        const last = lastContent[lastContent.length - 1];
        if (last && 'text' in last && last.type === 'text') {
          const updated = [...lastContent];
          updated[updated.length - 1] = { type: 'text', text: last.text + delta };
          this.streamingMessage = { ...this.streamingMessage, content: updated };
        } else {
          this.streamingMessage = {
            ...this.streamingMessage,
            content: [...lastContent, { type: 'text', text: delta }],
          };
        }
      } else {
        this.streamingMessage = { role: 'assistant', content: [{ type: 'text', text: delta }] };
      }
    } else if ('Error' in event) {
      const errMsg = event.Error as string;
      console.error('[chat] daemon event error:', errMsg);
      this.error = errMsg;
      this.isLoading = false;
      this.streamingMessage = null;
    }
  }

  // --- Cleanup ---

  /** Close session and unsubscribe. Safe to call from beforeunload (fire-and-forget). */
  cleanup() {
    this.unsubscribeEvents();
    if (this.currentSessionId) {
      // keepalive: true survives page navigation, unlike regular fetch
      const base = (typeof location !== 'undefined' && location.port !== '9800')
        ? 'http://localhost:9800' : '';
      fetch(`${base}/api/session/${this.currentSessionId}`, {
        method: 'DELETE',
        keepalive: true,
      }).catch(() => {});
      this.currentSessionId = null;
    }
  }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function toDisplayMessage(msg: ResolvedMessage): DisplayMessage {
  return {
    role: msg.role as 'user' | 'assistant' | 'system',
    content: msg.content,
    turnId: (msg as any).turn_id ?? msg.turnId,
  };
}

// ---------------------------------------------------------------------------
// Singleton
// ---------------------------------------------------------------------------

export const chatStore = new ChatStore();
