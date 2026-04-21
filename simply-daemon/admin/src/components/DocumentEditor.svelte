<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { EditorView, keymap, lineNumbers, highlightActiveLine, highlightActiveLineGutter } from '@codemirror/view';
  import { EditorState } from '@codemirror/state';
  import { markdown } from '@codemirror/lang-markdown';
  import { languages } from '@codemirror/language-data';
  import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
  import { syntaxHighlighting, defaultHighlightStyle, bracketMatching } from '@codemirror/language';
  import { oneDark } from '@codemirror/theme-one-dark';

  let { content = '', onsave }: { content: string; onsave: (content: string) => void } = $props();

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
        keymap.of([...defaultKeymap, ...historyKeymap]),
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
    // the user switches from Edit to Rendered mode before the debounce fires.
    if (saveTimeout !== undefined && view) {
      clearTimeout(saveTimeout);
      saveTimeout = undefined;
      onsave(view.state.doc.toString());
    }
    view?.destroy();
  });

  // Update editor content when prop changes externally
  $effect(() => {
    if (view && content !== view.state.doc.toString()) {
      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: content },
      });
    }
  });
</script>

<div bind:this={editorEl} class="h-full w-full overflow-hidden rounded border border-border"></div>
