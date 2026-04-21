// Documents store — list, selection, tab navigation, CRUD.
//
// Parallels chat.svelte.ts. Kept local to noema for now; promote into
// @simply/client once the two clients' document flows demonstrably agree.

import {
  getTransport,
  documentApi,
  type DocumentInfo,
  type DocumentDetail,
  type TabInfo,
  type Transport,
} from '@simply/client';

function setError(store: DocumentsStore, msg: string, e: unknown) {
  const full = `${msg}: ${e}`;
  console.error('[documents]', full, e);
  store.error = full;
}

class DocumentsStore {
  documents = $state<DocumentInfo[]>([]);
  selected = $state<DocumentDetail | null>(null);
  selectedTab = $state<TabInfo | null>(null);
  searchQuery = $state('');
  isLoading = $state(false);
  error = $state<string | null>(null);

  private transport!: Transport;
  private api!: ReturnType<typeof documentApi>;
  private initialized = false;
  private searchDebounce?: ReturnType<typeof setTimeout>;

  async init() {
    if (this.initialized) return;
    this.initialized = true;
    this.transport = getTransport();
    this.api = documentApi(this.transport);
    await this.reload();
  }

  async reload() {
    this.isLoading = true;
    try {
      this.documents = this.searchQuery.trim()
        ? await this.api.searchDocuments(this.searchQuery.trim())
        : await this.api.listDocuments();
    } catch (e) {
      setError(this, 'Failed to load documents', e);
    }
    this.isLoading = false;
  }

  /** Updates the search query and reloads with a short debounce. */
  setSearch(query: string) {
    this.searchQuery = query;
    clearTimeout(this.searchDebounce);
    this.searchDebounce = setTimeout(() => this.reload(), 300);
  }

  async select(documentId: string) {
    this.flushCurrentTab();
    try {
      const doc = await this.api.getDocument(documentId);
      this.selected = doc;
      this.selectedTab = doc.tabs[0] ?? null;
    } catch (e) {
      setError(this, 'Failed to load document', e);
    }
  }

  async reloadSelected() {
    if (!this.selected) return;
    const prevTabId = this.selectedTab?.id;
    try {
      const doc = await this.api.getDocument(this.selected.id);
      this.selected = doc;
      this.selectedTab = doc.tabs.find((t) => t.id === prevTabId) ?? doc.tabs[0] ?? null;
    } catch (e) {
      setError(this, 'Failed to reload document', e);
    }
  }

  setTab(tab: TabInfo) {
    this.flushCurrentTab();
    this.selectedTab = tab;
  }

  async create(title: string) {
    if (!title.trim()) return;
    try {
      const doc = await this.api.createDocument({
        title: title.trim(),
        documentType: null,
        content: null,
        sourceId: null,
      });
      await this.reload();
      await this.select(doc.id);
    } catch (e) {
      setError(this, 'Failed to create document', e);
    }
  }

  async remove(documentId: string) {
    try {
      await this.api.deleteDocument(documentId);
      if (this.selected?.id === documentId) {
        this.selected = null;
        this.selectedTab = null;
      }
      await this.reload();
    } catch (e) {
      setError(this, 'Failed to delete document', e);
    }
  }

  async rename(documentId: string, title: string) {
    try {
      await this.api.renameDocument(documentId, title);
      await this.reload();
      if (this.selected?.id === documentId) {
        this.selected = { ...this.selected, title };
      }
    } catch (e) {
      setError(this, 'Failed to rename document', e);
    }
  }

  /** Ask the daemon to embed any pending tab content immediately (normally
   * debounced). Called on tab switch + unload so edits don't sit un-embedded
   * when the user moves away. Fire-and-forget. */
  private flushCurrentTab() {
    const tab = this.selectedTab;
    if (!tab) return;
    this.transport
      .rpc('document.flush_tab_embedding', 'POST', `/api/document/tab/${tab.id}/flush`)
      .catch(() => {});
  }
}

export const documentsStore = new DocumentsStore();
