import type { ConversationInfo } from "../types";
import type { ActivityId } from "./ActivityBar";
import { ConversationsPanel } from "./panels/ConversationsPanel";
import { DocumentsPanel } from "./panels/DocumentsPanel";

interface SidePanelProps {
  activeActivity: ActivityId;
  conversations: ConversationInfo[];
  currentConversationId: string;
  onNewConversation: () => void;
  onSelectConversation: (id: string) => void;
  onDeleteConversation: (id: string) => void;
  onRenameConversation: (id: string, name: string) => void;
  // Document props
  selectedDocumentId?: string | null;
  onSelectDocument?: (docId: string) => void;
}

export function SidePanel({
  activeActivity,
  conversations,
  currentConversationId,
  onNewConversation,
  onSelectConversation,
  onDeleteConversation,
  onRenameConversation,
  selectedDocumentId,
  onSelectDocument,
}: SidePanelProps) {
  return (
    <div className="w-64 bg-surface border-r border-gray-700 flex flex-col h-full">
      {activeActivity === "conversations" && (
        <ConversationsPanel
          conversations={conversations}
          currentConversationId={currentConversationId}
          onNewConversation={onNewConversation}
          onSelectConversation={onSelectConversation}
          onDeleteConversation={onDeleteConversation}
          onRenameConversation={onRenameConversation}
        />
      )}
      {activeActivity === "documents" && (
        <DocumentsPanel
          selectedDocumentId={selectedDocumentId}
          onSelectDocument={onSelectDocument}
        />
      )}
    </div>
  );
}
