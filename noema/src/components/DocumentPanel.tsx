import { useState, useEffect } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import rehypeKatex from "rehype-katex";
import type { DocumentContentResponse, DocumentTabResponse } from "../generated";
import * as tauri from "../tauri";

interface DocumentPanelProps {
  documentId: string;
  onClose: () => void;
  /** When true, renders inline in the main content area instead of as an overlay */
  embedded?: boolean;
}

export function DocumentPanel({ documentId, onClose, embedded = false }: DocumentPanelProps) {
  const [document, setDocument] = useState<DocumentContentResponse | null>(null);
  const [selectedTabId, setSelectedTabId] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [syncing, setSyncing] = useState(false);

  useEffect(() => {
    loadDocument();
  }, [documentId]);

  const loadDocument = async () => {
    setLoading(true);
    setError(null);
    try {
      const doc = await tauri.getDocumentContent(documentId);
      setDocument(doc);
      // Select the first tab by default
      if (doc.tabs.length > 0 && !selectedTabId) {
        setSelectedTabId(doc.tabs[0].id);
      }
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  };

  const handleSync = async () => {
    if (!document) return;
    setSyncing(true);
    try {
      await tauri.syncGoogleDoc(documentId);
      await loadDocument();
    } catch (err) {
      setError(String(err));
    } finally {
      setSyncing(false);
    }
  };

  const formatDate = (timestamp: bigint) => {
    // Convert bigint unix timestamp to Date
    return new Date(Number(timestamp) * 1000).toLocaleString();
  };

  const selectedTab = document?.tabs.find(t => t.id === selectedTabId);

  // Build tab hierarchy for rendering
  const buildTabTree = (tabs: DocumentTabResponse[], parentId: string | null = null): DocumentTabResponse[] => {
    return tabs
      .filter(t => t.parentTabId === parentId)
      .sort((a, b) => a.tabIndex - b.tabIndex);
  };

  const renderTabButton = (tab: DocumentTabResponse, depth: number = 0) => {
    const isSelected = tab.id === selectedTabId;
    const children = buildTabTree(document?.tabs || [], tab.id);

    return (
      <div key={tab.id}>
        <button
          onClick={() => setSelectedTabId(tab.id)}
          className={`w-full text-left px-2 py-1.5 rounded text-sm transition-colors ${
            isSelected
              ? 'bg-teal-600/30 text-teal-300'
              : 'hover:bg-gray-700/50 text-muted hover:text-foreground'
          }`}
          style={{ paddingLeft: `${8 + depth * 16}px` }}
        >
          {tab.icon && <span className="mr-1">{tab.icon}</span>}
          {tab.title}
        </button>
        {children.map(child => renderTabButton(child, depth + 1))}
      </div>
    );
  };

  const rootTabs = document ? buildTabTree(document.tabs, null) : [];
  const hasTabs = document && document.tabs.length > 1;

  // Different container styles for embedded vs overlay mode
  const containerClass = embedded
    ? "h-full bg-surface flex flex-col"
    : "fixed inset-y-0 right-0 w-[600px] bg-surface border-l border-gray-700 flex flex-col shadow-xl z-50";

  return (
    <div className={containerClass}>
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3 border-b border-gray-700 bg-elevated">
        <div className="flex items-center gap-3 min-w-0">
          <svg className="w-5 h-5 text-teal-400 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
          </svg>
          <h2 className="text-lg font-semibold text-foreground truncate">
            {document?.info.title || 'Document'}
          </h2>
        </div>
        {!embedded && (
          <button
            onClick={onClose}
            className="p-1 hover:bg-gray-600 rounded transition-colors"
            aria-label="Close panel"
          >
            <svg className="w-5 h-5 text-muted" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        )}
      </div>

      {/* Toolbar */}
      {document && (
        <div className="flex items-center gap-2 px-4 py-2 border-b border-gray-700 bg-background text-sm">
          <span className="text-muted">
            Updated: {formatDate(document.info.updatedAt)}
          </span>
          {document.info.source === 'google_drive' && (
            <>
              <div className="flex-1" />
              <button
                onClick={handleSync}
                disabled={syncing}
                className="flex items-center gap-1 px-2 py-1 bg-teal-600 hover:bg-teal-500 disabled:opacity-50 text-white rounded text-xs transition-colors"
              >
                <svg className={`w-3 h-3 ${syncing ? 'animate-spin' : ''}`} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
                </svg>
                {syncing ? 'Syncing...' : 'Sync'}
              </button>
            </>
          )}
        </div>
      )}

      {/* Main content area */}
      <div className="flex-1 flex overflow-hidden">
        {/* Tab sidebar (only if multiple tabs) */}
        {hasTabs && (
          <div className="w-48 flex-shrink-0 border-r border-gray-700 overflow-y-auto bg-background p-2">
            <div className="text-xs text-muted uppercase tracking-wider px-2 py-1 mb-1">
              Tabs
            </div>
            {rootTabs.map(tab => renderTabButton(tab))}
          </div>
        )}

        {/* Content */}
        <div className="flex-1 overflow-y-auto p-4">
          {loading ? (
            <div className="flex items-center justify-center h-32">
              <div className="animate-spin w-6 h-6 border-2 border-teal-500 border-t-transparent rounded-full" />
            </div>
          ) : error ? (
            <div className="bg-red-900/30 text-red-300 p-4 rounded-lg">
              <p className="font-semibold">Error loading document</p>
              <p className="text-sm mt-1">{error}</p>
              <button
                onClick={loadDocument}
                className="mt-3 px-3 py-1 bg-red-600 hover:bg-red-500 text-white rounded text-sm transition-colors"
              >
                Retry
              </button>
            </div>
          ) : selectedTab ? (
            <div className="prose prose-sm prose-invert max-w-none">
              {hasTabs && (
                <h3 className="text-teal-400 mb-4">{selectedTab.title}</h3>
              )}
              <ReactMarkdown
                remarkPlugins={[remarkGfm, remarkMath]}
                rehypePlugins={[rehypeKatex]}
                // Allow noema-asset:// protocol URLs to pass through unchanged
                urlTransform={(url) => url}
                components={{
                  code(props) {
                    const { children, className } = props;
                    const isInline = !className;
                    return isInline ? (
                      <code className="bg-elevated text-gray-100 px-1 py-0.5 rounded text-sm">
                        {children}
                      </code>
                    ) : (
                      <code className={className}>{children}</code>
                    );
                  },
                  pre(props) {
                    return (
                      <pre className="bg-background text-gray-100 p-3 rounded-lg overflow-x-auto text-sm">
                        {props.children}
                      </pre>
                    );
                  },
                  img(props) {
                    const { src, alt } = props;
                    if (!src) {
                      return <span className="text-gray-500">[image]</span>;
                    }
                    return (
                      <img
                        src={src}
                        alt={alt || 'Document image'}
                        className="max-w-full h-auto rounded"
                        loading="lazy"
                        onError={(e) => {
                          console.error('Image load error:', src);
                          (e.target as HTMLImageElement).style.display = 'none';
                        }}
                      />
                    );
                  },
                }}
              >
                {selectedTab.contentMarkdown || '*(No content)*'}
              </ReactMarkdown>
            </div>
          ) : document && document.tabs.length === 0 ? (
            <p className="text-muted text-center">This document has no content</p>
          ) : (
            <p className="text-muted text-center">Select a tab to view content</p>
          )}
        </div>
      </div>
    </div>
  );
}
