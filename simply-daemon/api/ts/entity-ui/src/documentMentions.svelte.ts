import { entityApi, type EntitySummary, type InputContent, type Transport } from '@simply/client';

type EntityClient = ReturnType<typeof entityApi>;

export type MentionRange = {
  start: number;
  end: number;
  query: string;
};

export type MentionInsert = {
  value: string;
  cursor: number;
};

function asEntityClient(client: EntityClient | Transport): EntityClient {
  return typeof (client as EntityClient).listEntities === 'function'
    ? client as EntityClient
    : entityApi(client as Transport);
}

export function findDocumentMention(value: string, cursor: number): MentionRange | null {
  const before = value.slice(0, cursor);
  const at = before.lastIndexOf('@');
  if (at < 0) return null;

  const prefix = at === 0 ? '' : before[at - 1];
  if (prefix && !/\s/.test(prefix)) return null;

  const query = before.slice(at + 1);
  if (query.includes('\n')) return null;
  if (query.includes(')') || query.includes(']')) return null;
  if (query.length > 80) return null;

  return { start: at, end: cursor, query };
}

export function documentMentionTitle(title: string | null): string {
  return (title ?? '(untitled)').replace(/[\]\n\r]+/g, ' ').trim() || '(untitled)';
}

export function formatDocumentMention(doc: EntitySummary): string {
  return `@[${documentMentionTitle(doc.title)}](entity:${doc.id}) `;
}

export function parseDocumentMentionContent(value: string): InputContent[] {
  const content: InputContent[] = [];
  const token = /@\[([^\]]+)\]\(entity:([^)]+)\)/g;
  let cursor = 0;
  for (const match of value.matchAll(token)) {
    const index = match.index ?? 0;
    const before = value.slice(cursor, index);
    if (before.trim()) content.push({ type: 'text', text: before });
    content.push({ type: 'entityRef', id: match[2] });
    cursor = index + match[0].length;
  }

  const after = value.slice(cursor);
  if (after.trim()) content.push({ type: 'text', text: after });
  if (content.length === 0 && value.trim()) {
    content.push({ type: 'text', text: value.trim() });
  }
  return content;
}

export function documentMentionFuzzyScore(query: string, doc: EntitySummary): number | null {
  const title = doc.title ?? '(untitled)';
  const haystack = `${title} ${doc.kind} ${doc.origin ?? ''}`.toLowerCase();
  const needle = query.trim().toLowerCase();
  if (!needle) return doc.updatedAt / 1000000000;

  const direct = haystack.indexOf(needle);
  if (direct >= 0) return 1000 - direct + needle.length * 4;

  let score = 0;
  let cursor = 0;
  for (const ch of needle) {
    const found = haystack.indexOf(ch, cursor);
    if (found < 0) return null;
    score += found === cursor ? 8 : 3;
    cursor = found + 1;
  }
  return score;
}

export function createDocumentMentionController(client: EntityClient | Transport) {
  const api = asEntityClient(client);

  let documents = $state<EntitySummary[]>([]);
  let documentsLoaded = $state(false);
  let loading = $state(false);
  let open = $state(false);
  let query = $state('');
  let start = $state(0);
  let end = $state(0);
  let selectedIndex = $state(0);
  let matches = $state<EntitySummary[]>([]);

  function refreshMatches() {
    matches = documents
      .map((doc) => ({ doc, score: documentMentionFuzzyScore(query, doc) }))
      .filter((item): item is { doc: EntitySummary; score: number } => item.score !== null)
      .sort((a, b) => b.score - a.score || b.doc.updatedAt - a.doc.updatedAt)
      .slice(0, 8)
      .map((item) => item.doc);
    selectedIndex = Math.min(selectedIndex, Math.max(0, matches.length - 1));
  }

  async function loadDocuments() {
    if (documentsLoaded || loading) return;
    loading = true;
    try {
      documents = await api.listEntities('document::', null);
      documentsLoaded = true;
      refreshMatches();
    } catch (e) {
      console.error('[document-mentions] load failed:', e);
    } finally {
      loading = false;
    }
  }

  function update(value: string, cursor: number, disabled = false) {
    if (disabled) {
      open = false;
      return;
    }

    const active = findDocumentMention(value, cursor);
    if (!active) {
      open = false;
      return;
    }

    const queryChanged = active.query !== query;
    open = true;
    query = active.query;
    start = active.start;
    end = active.end;
    if (queryChanged) selectedIndex = 0;
    refreshMatches();
    void loadDocuments();
  }

  function close() {
    open = false;
  }

  function moveSelection(delta: number) {
    if (matches.length === 0) return;
    selectedIndex = (selectedIndex + delta + matches.length) % matches.length;
  }

  function setSelectedIndex(index: number) {
    selectedIndex = Math.max(0, Math.min(index, Math.max(0, matches.length - 1)));
  }

  function insertMention(doc: EntitySummary, value: string): MentionInsert {
    const token = formatDocumentMention(doc);
    const nextValue = value.slice(0, start) + token + value.slice(end);
    open = false;
    return { value: nextValue, cursor: start + token.length };
  }

  function insertSelected(value: string): MentionInsert | null {
    const selected = matches[selectedIndex];
    if (!selected) return null;
    return insertMention(selected, value);
  }

  return {
    get documents() { return documents; },
    get loading() { return loading; },
    get open() { return open; },
    get query() { return query; },
    get selectedIndex() { return selectedIndex; },
    get selectedMatch() { return matches[selectedIndex] ?? null; },
    get matches() { return matches; },
    update,
    close,
    moveSelection,
    setSelectedIndex,
    insertMention,
    insertSelected,
    parseContent: parseDocumentMentionContent,
  };
}

export type DocumentMentionController = ReturnType<typeof createDocumentMentionController>;
