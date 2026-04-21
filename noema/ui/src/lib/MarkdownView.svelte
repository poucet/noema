<script lang="ts">
  import { marked } from 'marked';
  import DOMPurify from 'dompurify';
  import katex from 'katex';
  import { getBaseUrl } from '@simply/client';

  type Props = { content: string };
  const { content }: Props = $props();

  let html = $state('');

  // Embedded blob links are written as `/api/blob/{hash}` — same-origin when
  // the daemon serves the UI, but in Tauri the webview origin is
  // `tauri://localhost` so we absolute-ize them against the daemon URL.
  function absolutizeApiUrls(src: string): string {
    const base = getBaseUrl();
    if (!base) return src;
    return src.replace(/(["'(])\/api\//g, (_, lead) => `${lead}${base}/api/`);
  }

  function renderMath(src: string): string {
    let out = src.replace(/\$\$([\s\S]+?)\$\$/g, (_, tex) => {
      try {
        return katex.renderToString(tex, { displayMode: true, throwOnError: false });
      } catch {
        return `<code>${escapeHtml(tex)}</code>`;
      }
    });
    out = out.replace(/(?<!\\)\$([^\n$]+?)\$/g, (_, tex) => {
      try {
        return katex.renderToString(tex, { displayMode: false, throwOnError: false });
      } catch {
        return `<code>${escapeHtml(tex)}</code>`;
      }
    });
    return out;
  }

  function escapeHtml(s: string): string {
    return s.replace(/[&<>"']/g, c => ({
      '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
    }[c]!));
  }

  $effect(() => {
    const withMath = renderMath(content ?? '');
    const rendered = marked.parse(withMath, { async: false, breaks: true, gfm: true }) as string;
    const absolutized = absolutizeApiUrls(rendered);
    html = DOMPurify.sanitize(absolutized, {
      ADD_TAGS: ['math', 'annotation', 'semantics', 'mrow', 'mi', 'mo', 'mn', 'msup', 'msub', 'mfrac', 'msqrt', 'mtext'],
      ADD_ATTR: ['aria-hidden', 'encoding', 'display'],
    });
  });
</script>

<div class="prose prose-invert max-w-none">
  {@html html}
</div>

<style>
  div :global(h1) { font-size: 1.75rem; font-weight: 600; margin: 1rem 0 0.5rem; }
  div :global(h2) { font-size: 1.4rem; font-weight: 600; margin: 1rem 0 0.5rem; }
  div :global(h3) { font-size: 1.15rem; font-weight: 600; margin: 0.75rem 0 0.4rem; }
  div :global(h4) { font-size: 1rem; font-weight: 600; margin: 0.75rem 0 0.4rem; }
  div :global(p) { margin: 0.5rem 0; line-height: 1.6; }
  div :global(ul), div :global(ol) { margin: 0.5rem 0; padding-left: 1.5rem; }
  div :global(li) { margin: 0.2rem 0; }
  div :global(a) { color: rgb(20 184 166); text-decoration: underline; }
  div :global(code) {
    background: rgba(255, 255, 255, 0.08);
    padding: 0.1rem 0.3rem;
    border-radius: 3px;
    font-size: 0.9em;
  }
  div :global(pre) {
    background: rgba(0, 0, 0, 0.3);
    padding: 0.75rem;
    border-radius: 4px;
    overflow-x: auto;
    margin: 0.75rem 0;
  }
  div :global(pre code) { background: transparent; padding: 0; }
  div :global(blockquote) {
    border-left: 3px solid rgba(255, 255, 255, 0.2);
    padding-left: 0.75rem;
    margin: 0.5rem 0;
    color: rgba(255, 255, 255, 0.7);
  }
  div :global(img) {
    max-width: 100%;
    height: auto;
    border-radius: 4px;
    margin: 0.5rem 0;
  }
  div :global(table) {
    border-collapse: collapse;
    margin: 0.75rem 0;
  }
  div :global(th), div :global(td) {
    border: 1px solid rgba(255, 255, 255, 0.15);
    padding: 0.3rem 0.6rem;
  }
  div :global(hr) {
    border: 0;
    border-top: 1px solid rgba(255, 255, 255, 0.15);
    margin: 1rem 0;
  }
</style>
