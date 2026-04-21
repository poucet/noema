// Shared entity-viewer UI primitives used by admin and Noema.
//
// Top-level shell (for side-by-side list + detail layouts — admin):
export { default as EntityBrowser } from './EntityBrowser.svelte';

// Split halves (for layouts where list + detail live in different parts
// of the page — Noema). Each takes a shared `EntitiesBrowser` instance.
export { default as EntitySidebar } from './EntitySidebar.svelte';
export { default as EntityPane } from './EntityPane.svelte';

// Lowest-level primitives (for fully custom composition):
export { default as MarkdownView } from './MarkdownView.svelte';
export { default as DocumentEditor } from './DocumentEditor.svelte';
export { default as EntityList } from './EntityList.svelte';
export { default as EntityDetail } from './EntityDetail.svelte';

// Shared browser state (list + selection + content loading + create/delete).
export {
  createEntitiesBrowser,
  DEFAULT_NEW_KIND,
  CONTAINED_IN,
  type EntitiesBrowser,
  type EntityClient,
} from './useEntitiesBrowser.svelte';

// Re-export the entity wire types so consumers can import from one place.
export type {
  EntitySummary,
  RelatedEntity,
  EntityContent,
  CreateEntityRequest,
  UpdateEntityContentRequest,
  AddRelationRequest,
  MoveRelationRequest,
} from '@simply/client';
