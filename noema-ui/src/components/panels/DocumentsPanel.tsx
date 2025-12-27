import { useState, useEffect } from "react";
import type { DocumentInfoResponse } from "../../generated";
import * as tauri from "../../tauri";

interface DocumentsPanelProps {
  onSelectDocument?: (docId: string) => void;
  selectedDocumentId?: string | null;
}

function formatDate(timestamp: number | bigint): string {
  const date = new Date(Number(timestamp) * 1000); // Convert unix timestamp to ms
  const now = new Date();
  const diffDays = Math.floor(
    (now.getTime() - date.getTime()) / (1000 * 60 * 60 * 24)
  );

  if (diffDays === 0) return "Today";
  if (diffDays === 1) return "Yesterday";
  if (diffDays < 7) return `${diffDays} days ago`;
  return date.toLocaleDateString();
}

function getSourceIcon(source: string) {
  if (source === "google_drive") {
    return (
      <svg className="w-4 h-4" viewBox="0 0 87.3 78" fill="currentColor">
        <path d="m6.6 66.85 3.85 6.65c.8 1.4 1.95 2.5 3.3 3.3l13.75-23.8h-27.5c0 1.55.4 3.1 1.2 4.5z" fill="#0066da"/>
        <path d="m43.65 25-13.75-23.8c-1.35.8-2.5 1.9-3.3 3.3l-25.4 44a9.06 9.06 0 0 0 -1.2 4.5h27.5z" fill="#00ac47"/>
        <path d="m73.55 76.8c1.35-.8 2.5-1.9 3.3-3.3l1.6-2.75 7.65-13.25c.8-1.4 1.2-2.95 1.2-4.5h-27.502l5.852 11.5z" fill="#ea4335"/>
        <path d="m43.65 25 13.75-23.8c-1.35-.8-2.9-1.2-4.5-1.2h-18.5c-1.6 0-3.15.45-4.5 1.2z" fill="#00832d"/>
        <path d="m59.8 53h-32.3l-13.75 23.8c1.35.8 2.9 1.2 4.5 1.2h50.8c1.6 0 3.15-.45 4.5-1.2z" fill="#2684fc"/>
        <path d="m73.4 26.5-12.7-22c-.8-1.4-1.95-2.5-3.3-3.3l-13.75 23.8 16.15 28h27.45c0-1.55-.4-3.1-1.2-4.5z" fill="#ffba00"/>
      </svg>
    );
  }
  return (
    <svg className="w-4 h-4 text-muted" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
    </svg>
  );
}

export function DocumentsPanel({ onSelectDocument, selectedDocumentId }: DocumentsPanelProps) {
  const [documents, setDocuments] = useState<DocumentInfoResponse[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [showImportModal, setShowImportModal] = useState(false);

  useEffect(() => {
    loadDocuments();
  }, []);

  const loadDocuments = async () => {
    setLoading(true);
    setError(null);
    try {
      const docs = await tauri.listDocuments();
      setDocuments(docs);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  };

  const handleDelete = async (docId: string, e: React.MouseEvent) => {
    e.stopPropagation();
    if (!confirm("Delete this document?")) return;

    try {
      await tauri.deleteDocument(docId);
      setDocuments(docs => docs.filter(d => d.id !== docId));
    } catch (err) {
      setError(String(err));
    }
  };

  const handleImportComplete = () => {
    setShowImportModal(false);
    loadDocuments();
  };

  return (
    <>
      {/* Header */}
      <div className="p-4 border-b border-gray-700">
        <button
          onClick={() => setShowImportModal(true)}
          className="w-full px-4 py-2 bg-teal-600 hover:bg-teal-700 text-white rounded-lg font-medium transition-colors flex items-center justify-center gap-2"
        >
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
          </svg>
          Import Doc
        </button>
      </div>

      {/* Documents list */}
      <div className="flex-1 overflow-y-auto">
        {loading ? (
          <div className="flex items-center justify-center p-8">
            <div className="animate-spin w-6 h-6 border-2 border-teal-500 border-t-transparent rounded-full" />
          </div>
        ) : error ? (
          <div className="p-4 text-red-400 text-sm">
            <p>{error}</p>
            <button
              onClick={loadDocuments}
              className="mt-2 text-teal-400 hover:text-teal-300"
            >
              Retry
            </button>
          </div>
        ) : documents.length === 0 ? (
          <div className="text-center p-8">
            <svg
              className="w-12 h-12 text-muted mx-auto mb-3"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={1.5}
                d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
              />
            </svg>
            <p className="text-muted text-sm mb-2">No documents yet</p>
            <p className="text-muted text-xs">Click "Import Doc" to get started</p>
          </div>
        ) : (
          <ul className="py-2">
            {documents.map((doc) => {
              const isSelected = doc.id === selectedDocumentId;
              return (
                <li key={doc.id} className="px-2">
                  <button
                    onClick={() => onSelectDocument?.(doc.id)}
                    className={`w-full text-left p-3 rounded-lg transition-colors group ${
                      isSelected
                        ? "bg-teal-900/50 text-teal-100"
                        : "hover:bg-elevated text-gray-300"
                    }`}
                  >
                    <div className="flex items-center gap-2">
                      {getSourceIcon(doc.source)}
                      <span className="truncate font-medium text-sm flex-1">
                        {doc.title}
                      </span>
                      <button
                        onClick={(e) => handleDelete(doc.id, e)}
                        className="opacity-0 group-hover:opacity-100 p-1 text-muted hover:text-red-500 transition-opacity"
                        title="Delete"
                      >
                        <svg
                          className="w-4 h-4"
                          fill="none"
                          stroke="currentColor"
                          viewBox="0 0 24 24"
                        >
                          <path
                            strokeLinecap="round"
                            strokeLinejoin="round"
                            strokeWidth={2}
                            d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"
                          />
                        </svg>
                      </button>
                    </div>
                    <span className="text-xs text-muted">
                      {formatDate(doc.updatedAt)}
                    </span>
                  </button>
                </li>
              );
            })}
          </ul>
        )}
      </div>

      {/* Import Modal */}
      {showImportModal && (
        <ImportGoogleDocModal
          onClose={() => setShowImportModal(false)}
          onImported={handleImportComplete}
        />
      )}
    </>
  );
}

interface ImportModalProps {
  onClose: () => void;
  onImported: () => void;
}

function ImportGoogleDocModal({ onClose, onImported }: ImportModalProps) {
  const [googleDocs, setGoogleDocs] = useState<tauri.GoogleDocListItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [importing, setImporting] = useState<string | null>(null);

  useEffect(() => {
    loadGoogleDocs();
  }, []);

  const loadGoogleDocs = async (query?: string) => {
    setLoading(true);
    setError(null);
    try {
      const docs = await tauri.listGoogleDocs(query, 50);
      setGoogleDocs(docs);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  };

  const handleSearch = (e: React.FormEvent) => {
    e.preventDefault();
    loadGoogleDocs(searchQuery || undefined);
  };

  const handleImport = async (docId: string) => {
    setImporting(docId);
    setError(null);
    try {
      await tauri.importGoogleDoc(docId);
      onImported();
    } catch (err) {
      setError(String(err));
      setImporting(null);
    }
  };

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div className="bg-surface rounded-lg w-full max-w-lg max-h-[80vh] flex flex-col mx-4 shadow-xl">
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-gray-700">
          <h3 className="text-lg font-semibold text-foreground">Import from Google Docs</h3>
          <button
            onClick={onClose}
            className="p-1 hover:bg-gray-600 rounded transition-colors"
          >
            <svg className="w-5 h-5 text-muted" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        {/* Search */}
        <form onSubmit={handleSearch} className="p-4 border-b border-gray-700">
          <div className="flex gap-2">
            <input
              type="text"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder="Search documents..."
              className="flex-1 px-3 py-2 bg-background border border-gray-600 rounded-lg text-foreground placeholder-muted focus:outline-none focus:ring-2 focus:ring-teal-500"
            />
            <button
              type="submit"
              className="px-4 py-2 bg-teal-600 hover:bg-teal-700 text-white rounded-lg transition-colors"
            >
              Search
            </button>
          </div>
        </form>

        {/* Document list */}
        <div className="flex-1 overflow-y-auto p-4">
          {loading ? (
            <div className="flex items-center justify-center py-8">
              <div className="animate-spin w-6 h-6 border-2 border-teal-500 border-t-transparent rounded-full" />
            </div>
          ) : error ? (
            <div className="text-red-400 text-sm p-4 bg-red-900/20 rounded-lg">
              <p className="font-semibold mb-1">Error loading documents</p>
              <p>{error}</p>
              <button
                onClick={() => loadGoogleDocs(searchQuery || undefined)}
                className="mt-2 text-teal-400 hover:text-teal-300"
              >
                Retry
              </button>
            </div>
          ) : googleDocs.length === 0 ? (
            <p className="text-muted text-center py-8">
              No documents found. Make sure you're authenticated with Google Docs.
            </p>
          ) : (
            <ul className="space-y-2">
              {googleDocs.map((doc) => (
                <li key={doc.id}>
                  <button
                    onClick={() => handleImport(doc.id)}
                    disabled={importing !== null}
                    className="w-full text-left p-3 bg-background hover:bg-elevated rounded-lg transition-colors disabled:opacity-50 flex items-center gap-3"
                  >
                    <svg className="w-5 h-5 text-blue-400 flex-shrink-0" viewBox="0 0 87.3 78" fill="currentColor">
                      <path d="m6.6 66.85 3.85 6.65c.8 1.4 1.95 2.5 3.3 3.3l13.75-23.8h-27.5c0 1.55.4 3.1 1.2 4.5z" fill="#0066da"/>
                      <path d="m43.65 25-13.75-23.8c-1.35.8-2.5 1.9-3.3 3.3l-25.4 44a9.06 9.06 0 0 0 -1.2 4.5h27.5z" fill="#00ac47"/>
                      <path d="m73.55 76.8c1.35-.8 2.5-1.9 3.3-3.3l1.6-2.75 7.65-13.25c.8-1.4 1.2-2.95 1.2-4.5h-27.502l5.852 11.5z" fill="#ea4335"/>
                      <path d="m43.65 25 13.75-23.8c-1.35-.8-2.9-1.2-4.5-1.2h-18.5c-1.6 0-3.15.45-4.5 1.2z" fill="#00832d"/>
                      <path d="m59.8 53h-32.3l-13.75 23.8c1.35.8 2.9 1.2 4.5 1.2h50.8c1.6 0 3.15-.45 4.5-1.2z" fill="#2684fc"/>
                      <path d="m73.4 26.5-12.7-22c-.8-1.4-1.95-2.5-3.3-3.3l-13.75 23.8 16.15 28h27.45c0-1.55-.4-3.1-1.2-4.5z" fill="#ffba00"/>
                    </svg>
                    <div className="flex-1 min-w-0">
                      <p className="font-medium text-foreground truncate">{doc.name}</p>
                      {doc.modifiedTime && (
                        <p className="text-xs text-muted">
                          Modified: {new Date(doc.modifiedTime).toLocaleDateString()}
                        </p>
                      )}
                    </div>
                    {importing === doc.id ? (
                      <div className="animate-spin w-5 h-5 border-2 border-teal-500 border-t-transparent rounded-full" />
                    ) : (
                      <svg className="w-5 h-5 text-muted" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
                      </svg>
                    )}
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>
    </div>
  );
}
