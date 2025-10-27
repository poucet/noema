#!/bin/bash

# Git Worktree Manager for repositories with submodules
# Usage: ./worktree-manager.sh <command> <worktree-name>

set -e

SUBMODULES=("cli" "llm")
# Get the absolute path to the repository root
REPO_ROOT="$(git rev-parse --show-toplevel)"
WORKTREE_DIR="$(dirname "$REPO_ROOT")/athena-worktrees"

show_usage() {
    cat << EOF
Usage: $0 <command> <worktree-name> [branch-name]

Commands:
  create    Create a new worktree with submodule worktrees and open in VSCode
  remove    Remove a worktree and its submodule worktrees
  open      Open an existing worktree in VSCode
  list      List all worktrees

Examples:
  $0 create feature-1                    # Create from current branch and open in VSCode
  $0 create feature-1 my-branch          # Create from specific branch and open in VSCode
  $0 open feature-1                      # Open existing worktree in VSCode
  $0 remove feature-1
  $0 list
EOF
    exit 1
}

create_worktree() {
    local WORKTREE_NAME=$1
    local BRANCH_NAME=${2:-$WORKTREE_NAME}
    local WORKTREE_PATH="$WORKTREE_DIR/$WORKTREE_NAME"

    echo "Creating worktree: $WORKTREE_NAME"
    
    # Create directory if it doesn't exist
    mkdir -p "$WORKTREE_DIR"
    
    # Create main worktree
    echo "→ Creating main worktree at $WORKTREE_PATH"
    git worktree add "$WORKTREE_PATH" "$BRANCH_NAME" 2>/dev/null || \
        git worktree add -b "$BRANCH_NAME" "$WORKTREE_PATH"
    
    # Create submodule worktrees
    for submodule in "${SUBMODULES[@]}"; do
        if [ -d "$submodule/.git" ] || [ -f "$submodule/.git" ]; then
            echo "→ Creating worktree for submodule: $submodule"
            
            local SUB_BRANCH="$BRANCH_NAME-$submodule"
            local SUB_PATH="$WORKTREE_PATH/$submodule"
            
            cd "$submodule"
            
            # Create worktree (create branch if it doesn't exist)
            git worktree add "$SUB_PATH" "$SUB_BRANCH" 2>/dev/null || \
                git worktree add -b "$SUB_BRANCH" "$SUB_PATH"
            
            cd - > /dev/null
        else
            echo "⚠ Warning: Submodule $submodule not initialized"
        fi
    done
    
    # Create VSCode workspace file
    create_vscode_workspace "$WORKTREE_NAME" "$WORKTREE_PATH"
    
    echo "✓ Worktree created successfully at $WORKTREE_PATH"
    
    # Open in VSCode
    open_in_vscode "$WORKTREE_NAME"
}

create_vscode_workspace() {
    local WORKTREE_NAME=$1
    local WORKTREE_PATH=$2
    local WORKSPACE_FILE="$WORKTREE_PATH/$WORKTREE_NAME.code-workspace"
    
    echo "→ Creating VSCode workspace file"
    
    cat > "$WORKSPACE_FILE" << EOF
{
    "folders": [
        {
            "name": "athena (root)",
            "path": "."
        },
        {
            "name": "cli",
            "path": "cli"
        },
        {
            "name": "llm",
            "path": "llm"
        }
    ],
    "settings": {
        "git.enabled": true,
        "git.detectSubmodules": true,
        "files.watcherExclude": {
            "**/.git/objects/**": true,
            "**/.git/subtree-cache/**": true,
            "**/node_modules/**": true
        }
    }
}
EOF
}

open_in_vscode() {
    local WORKTREE_NAME=$1
    local WORKTREE_PATH="$WORKTREE_DIR/$WORKTREE_NAME"
    local WORKSPACE_FILE="$WORKTREE_PATH/$WORKTREE_NAME.code-workspace"
    
    if [ ! -d "$WORKTREE_PATH" ]; then
        echo "Error: Worktree $WORKTREE_NAME not found at $WORKTREE_PATH"
        exit 1
    fi
    
    # Check if VSCode is available
    if command -v code &> /dev/null; then
        echo "→ Opening in VSCode..."
        code "$WORKSPACE_FILE"
        echo "✓ VSCode opened with workspace: $WORKTREE_NAME"
    elif command -v code-insiders &> /dev/null; then
        echo "→ Opening in VSCode Insiders..."
        code-insiders "$WORKSPACE_FILE"
        echo "✓ VSCode Insiders opened with workspace: $WORKTREE_NAME"
    else
        echo "⚠ VSCode not found in PATH. You can manually open:"
        echo "   $WORKSPACE_FILE"
    fi
}

remove_worktree() {
    local WORKTREE_NAME=$1
    local WORKTREE_PATH="$WORKTREE_DIR/$WORKTREE_NAME"
    
    if [ ! -d "$WORKTREE_PATH" ]; then
        echo "Error: Worktree $WORKTREE_NAME not found at $WORKTREE_PATH"
        exit 1
    fi
    
    echo "Removing worktree: $WORKTREE_NAME"
    
    # Remove submodule worktrees first
    for submodule in "${SUBMODULES[@]}"; do
        if [ -d "$submodule/.git" ] || [ -f "$submodule/.git" ]; then
            local SUB_PATH="$WORKTREE_PATH/$submodule"
            
            if [ -d "$SUB_PATH" ]; then
                echo "→ Removing worktree for submodule: $submodule"
                cd "$submodule"
                git worktree remove "$SUB_PATH" --force 2>/dev/null || true
                cd - > /dev/null
            fi
        fi
    done
    
    # Remove main worktree
    echo "→ Removing main worktree"
    git worktree remove "$WORKTREE_PATH" --force
    
    echo "✓ Worktree removed successfully"
    echo "⚠ Remember to close the VSCode window for this worktree"
}

list_worktrees() {
    echo "Main repository worktrees:"
    git worktree list
    
    echo ""
    echo "Available workspaces:"
    if [ -d "$WORKTREE_DIR" ]; then
        for dir in "$WORKTREE_DIR"/*; do
            if [ -d "$dir" ]; then
                local name=$(basename "$dir")
                local workspace="$dir/$name.code-workspace"
                if [ -f "$workspace" ]; then
                    echo "  • $name (workspace: $workspace)"
                fi
            fi
        done
    fi
    
    echo ""
    for submodule in "${SUBMODULES[@]}"; do
        if [ -d "$submodule/.git" ] || [ -f "$submodule/.git" ]; then
            echo "Submodule '$submodule' worktrees:"
            cd "$submodule"
            git worktree list
            cd - > /dev/null
            echo ""
        fi
    done
}

# Main script
if [ $# -lt 1 ]; then
    show_usage
fi

COMMAND=$1
shift

case "$COMMAND" in
    create)
        if [ $# -lt 1 ]; then
            show_usage
        fi
        create_worktree "$@"
        ;;
    remove)
        if [ $# -lt 1 ]; then
            show_usage
        fi
        remove_worktree "$1"
        ;;
    open)
        if [ $# -lt 1 ]; then
            show_usage
        fi
        open_in_vscode "$1"
        ;;
    list)
        list_worktrees
        ;;
    *)
        echo "Error: Unknown command '$COMMAND'"
        show_usage
        ;;
esac