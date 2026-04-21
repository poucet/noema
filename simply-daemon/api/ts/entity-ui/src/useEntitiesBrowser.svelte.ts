// Shared state + behaviour for the entity-browser shell. Admin and Noema
// both mount EntityList + EntityDetail around this store; the browser takes
// the generated `entityApi(transport)` client directly — no adapter.

import { entityApi, type Transport } from '@simply/client';

/** The generated entity client shape. */
export type EntityClient = ReturnType<typeof entityApi>;

/** Default kind for the `+ New note` action. Override per-app if needed. */
export const DEFAULT_NEW_KIND = 'document::note';

/** Relation used to wire a container's contents together (tabs inside a
 *  tabbed doc, files inside a directory). Convention: `child → parent`, so
 *  listing a container's contents is an *incoming* query. */
export const CONTAINED_IN = 'structure::contained_in';

export function createEntitiesBrowser(client: EntityClient | Transport) {
  // Accept either the already-bound client or a transport (for convenience).
  const api: EntityClient =
    typeof (client as EntityClient).listEntities === 'function'
      ? (client as EntityClient)
      : entityApi(client as Transport);

  type EntitySummary = Awaited<ReturnType<EntityClient['getEntity']>>;
  type RelatedEntity = Awaited<ReturnType<EntityClient['listIncoming']>>[number];
  type EntityContent = Awaited<ReturnType<EntityClient['getEntityContent']>>;

  let entities = $state<EntitySummary[]>([]);
  let loading = $state(true);
  let searchQuery = $state('');
  let typeFilter = $state('document::');

  let selected = $state<EntitySummary | null>(null);
  /** Entities contained inside `selected` — rendered as the tab sidebar. */
  let contained = $state<RelatedEntity[]>([]);
  let viewEntity = $state<EntitySummary | null>(null);
  let viewContent = $state<EntityContent | null>(null);

  let creating = $state(false);
  let newEntityTitle = $state('');
  let creatingChild = $state(false);

  /** If true, the next EntityDetail mount should default to edit mode (used
   *  after `+ New note` so the user lands in the editor immediately). */
  let editOnMount = $state(false);

  function flushCurrent() {
    if (viewEntity) {
      api.flushEntityEmbedding(viewEntity.id).catch(() => {});
    }
  }

  async function load() {
    loading = true;
    try {
      const prefix = typeFilter || null;
      // `root_of_relation = contained_in` hides entities that live inside
      // another one (tabs of a tabbed doc, files inside a folder) — they're
      // reachable by drilling into their container.
      entities = searchQuery
        ? await api.searchEntities(searchQuery, prefix, CONTAINED_IN)
        : await api.listEntities(prefix, CONTAINED_IN);
    } catch (e) {
      console.error('Failed to load entities:', e);
    }
    loading = false;
  }

  async function selectEntity(id: string) {
    flushCurrent();
    const entity = entities.find(e => e.id === id) ?? null;
    selected = entity;
    contained = [];
    viewEntity = null;
    viewContent = null;
    creatingChild = false;
    editOnMount = false;
    if (!entity) return;

    const hasContained = (entity.incomingCounts[CONTAINED_IN] ?? 0) > 0;
    if (hasContained) {
      try {
        contained = await api.listIncoming(entity.id, CONTAINED_IN);
        if (contained.length > 0) await openChild(contained[0].summary);
      } catch (e) {
        console.error('Failed to load contained entities:', e);
      }
    } else if (entity.hasContent) {
      viewEntity = entity;
      await loadContent(entity.id);
    } else {
      viewEntity = entity;
    }
  }

  async function openChild(child: EntitySummary) {
    flushCurrent();
    viewEntity = child;
    viewContent = null;
    editOnMount = false;
    await loadContent(child.id);
  }

  async function loadContent(id: string) {
    try {
      viewContent = await api.getEntityContent(id);
    } catch (e) {
      console.error('Failed to load content:', e);
      viewContent = null;
    }
  }

  async function createEntity(title: string) {
    try {
      const created = await api.createEntity({
        kind: DEFAULT_NEW_KIND,
        title,
        content: '',
        origin: null,
        referencedAssets: [],
      });
      newEntityTitle = '';
      creating = false;
      await load();
      editOnMount = true;
      await selectEntity(created.id);
    } catch (e) {
      console.error('Failed to create entity:', e);
    }
  }

  async function deleteEntity(id: string) {
    if (!confirm('Delete this entity and everything it contains?')) return;
    try {
      await api.deleteEntity(id);
      if (selected?.id === id) {
        selected = null;
        contained = [];
        viewEntity = null;
        viewContent = null;
      }
      await load();
    } catch (e) {
      console.error('Failed to delete entity:', e);
    }
  }

  async function saveContent(content: string) {
    if (!viewEntity) return;
    try {
      await api.updateEntityContent(viewEntity.id, {
        content,
        referencedAssets: viewContent?.referencedAssets ?? [],
      });
      if (viewContent) viewContent = { ...viewContent, contentMarkdown: content };
    } catch (e) {
      console.error('Failed to save content:', e);
    }
  }

  async function createChild(title: string) {
    if (!selected) return;
    try {
      const tab = await api.createEntity({
        kind: 'document::tab',
        title,
        content: '',
        origin: null,
        referencedAssets: [],
      });
      // `contained_in` goes child → parent, so new tab is `from`, container is `to`.
      await api.addRelation({
        fromId: tab.id,
        toId: selected.id,
        relation: CONTAINED_IN,
        position: contained.length,
      });
      creatingChild = false;
      contained = await api.listIncoming(selected.id, CONTAINED_IN);
      editOnMount = true;
      await openChild(tab);
    } catch (e) {
      console.error('Failed to create child:', e);
    }
  }

  async function deleteChild(childId: string) {
    if (!selected) return;
    if (!confirm('Delete this tab?')) return;
    try {
      await api.deleteEntity(childId);
      contained = await api.listIncoming(selected.id, CONTAINED_IN);
      if (viewEntity?.id === childId) {
        viewEntity = contained[0]?.summary ?? null;
        viewContent = null;
        if (viewEntity) await loadContent(viewEntity.id);
      }
    } catch (e) {
      console.error('Failed to delete child:', e);
    }
  }

  return {
    // State (read-only for consumers)
    get entities() { return entities; },
    get loading() { return loading; },
    get selected() { return selected; },
    get contained() { return contained; },
    get viewEntity() { return viewEntity; },
    get viewContent() { return viewContent; },
    get creating() { return creating; },
    get creatingChild() { return creatingChild; },
    get newEntityTitle() { return newEntityTitle; },
    get editOnMount() { return editOnMount; },
    get searchQuery() { return searchQuery; },
    get typeFilter() { return typeFilter; },
    // Actions
    load,
    selectEntity,
    openChild,
    createEntity,
    deleteEntity,
    saveContent,
    createChild,
    deleteChild,
    flushCurrent,
    // Mutators for inputs bound to the list UI
    setSearchQuery(q: string) { searchQuery = q; load(); },
    setTypeFilter(id: string) { typeFilter = id; load(); },
    startCreate() { creating = true; newEntityTitle = ''; },
    cancelCreate() { creating = false; newEntityTitle = ''; },
    setNewEntityTitle(v: string) { newEntityTitle = v; },
    startCreateChild() { creatingChild = true; },
    cancelCreateChild() { creatingChild = false; },
  };
}

export type EntitiesBrowser = ReturnType<typeof createEntitiesBrowser>;
