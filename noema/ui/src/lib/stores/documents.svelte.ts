// Documents store — a single shared `EntitiesBrowser` bound to Noema's
// transport. Used by both the sidebar (DocumentsPanel) and the main pane
// (DocumentView) so selection/state stays in sync.

import { getTransport, entityApi } from '@simply/client';
import { createEntitiesBrowser, type EntitiesBrowser } from '@simply/entity-ui';

let _browser: EntitiesBrowser | null = null;

export function documentsBrowser(): EntitiesBrowser {
  if (!_browser) _browser = createEntitiesBrowser(entityApi(getTransport()));
  return _browser;
}
