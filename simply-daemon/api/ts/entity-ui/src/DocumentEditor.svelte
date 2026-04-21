<!--
  Shared markdown editor used by admin and Noema.

  Debounced autosave (1 s). On unmount any pending save is flushed
  synchronously so mode switches never drop edits.
-->
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { EditorView, keymap, lineNumbers, highlightActiveLine, highlightActiveLineGutter } from '@codemirror/view';
  import { EditorState } from '@codemirror/state';
  import { markdown } from '@codemirror/lang-markdown';
  import { languages } from '@codemirror/language-data';
  import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
  import { syntaxHighlighting, defaultHighlightStyle, bracketMatching } from '@codemirror/language';
  import { oneDark } from '@codemirror/theme-one-dark';

  type Props = {
    content: string;
    onsave: (content: string) => void;
    /** Called when the user presses ⌘+Enter / Ctrl+Enter to commit explicitly. */
    oncommit?: () => void;
  };
  let { content = '', onsave, oncommit }: Props = $props();

  let editorEl: HTMLDivElement;
  let view: EditorView | undefined;
  let saveTimeout: ReturnType<typeof setTimeout> | undefined;

  function debouncedSave(doc: string) {
    clearTimeout(saveTimeout);
    saveTimeout = setTimeout(() => onsave(doc), 1000);
  }

  onMount(() => {
    const state = EditorState.create({
      doc: content,
      extensions: [
        lineNumbers(),
        highlightActiveLine(),
        highlightActiveLineGutter(),
        history(),
        bracketMatching(),
        syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
        markdown({ codeLanguages: languages }),
        oneDark,
        keymap.of([
          {
            key: 'Mod-Enter',
            run: () => {
              if (view) {
                clearTimeout(saveTimeout);
                saveTimeout = undefined;
                onsave(view.state.doc.toString());
              }
              oncommit?.();
              return true;
            },
          },
          ...defaultKeymap,
          ...historyKeymap,
        ]),
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            debouncedSave(update.state.doc.toString());
          }
        }),
        EditorView.theme({
          '&': { height: '100%', fontSize: '14px' },
          '.cm-scroller': { overflow: 'auto' },
          '.cm-content': { fontFamily: 'ui-monospace, SFMono-Regular, Menlo, monospace' },
        }),
      ],
    });

    view = new EditorView({ state, parent: editorEl });
  });

  onDestroy(() => {
    // Flush any pending save before tearing down so edits aren't lost when
    // the user switches out of edit mode before the debounce fires.
    if (saveTimeout !== undefined && view) {
      clearTimeout(saveTimeout);
      saveTimeout = undefined;
      onsave(view.state.doc.toString());
    }
    view?.destroy();
  });

  // Keep the editor in sync when the content prop changes externally (e.g. the
  // caller loads a different entity while the editor is mounted).
  $effect(() => {
    if (view && content !== view.state.doc.toString()) {
      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: content },
      });
    }
  });
</script>

<div bind:this={editorEl} class="h-full w-full overflow-hidden rounded border border-border"></div>
