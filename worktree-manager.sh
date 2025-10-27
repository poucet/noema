#!/bin/bash

# Git Worktree Manager for repositories with submodules
# Usage: ./worktree-manager.sh <command> <worktree-name>

set -e

# Get the absolute path to the repository root
REPO_ROOT="$(git rev-parse --show-toplevel)"
WORKTREE_DIR=$(realpath "$(dirname "$REPO_ROOT")/athena-worktrees")

# Dynamically discover submodules
get_submodules() {
    # Run from inside the REPO_ROOT
    (cd "$REPO_ROOT" && git config --file .gitmodules --get-regexp path | awk '{print $2}' || true)
}

# Cache submodules for the script execution
SUBMODULES=($(get_submodules))

show_usage() {
    cat << EOF
Usage: $0 <command> <worktree-name> [branch-name|target-branch]

Commands:
  create    Create a new worktree, initialize submodules, and open in VSCode
  remove    Remove a worktree (checks if merged)
  open      Open an existing worktree in VSCode
  merge     Merge worktree branch back into target branch (default: main)
  sync      Sync worktree with target branch - pull latest changes (default: main)
  pull      Pull latest changes from remote into current worktree (default: main)
  push      Push target branch (and its submodules) to origin (default: main)
  list      List all worktrees
  setup     Enable tab-completion for current shell session
  complete  Internal command for shell completion (use: complete commands|worktrees)

Examples:
  $0 create feature-1                    # Create 'feature-1' branch from current branch
  $0 create feature-1 my-branch          # Create 'my-branch' branch from current branch
  $0 open feature-1                      # Open existing worktree in VSCode
  $0 merge feature-1                     # Merge feature-1's branch into main
  $0 merge feature-1 develop             # Merge feature-1's branch into develop
  $0 sync feature-1                      # Sync feature-1 worktree with latest main
  $0 sync feature-1 develop              # Sync feature-1 worktree with latest develop
  $0 pull                                # From inside a worktree: pull latest main
  $0 pull develop                        # From inside a worktree: pull latest develop
  $0 push main                           # Push main branch (and submodules) to origin
  $0 push develop                        # Push develop branch (and submodules) to origin
  $0 remove feature-1                    # Remove worktree (warns if not merged)
  $0 list

Bash Completion:
  For current shell session only:
    source <($0 setup)

  To enable permanently, add this to your ~/.bashrc:
    source <($0 setup)
EOF
    exit 1
}

# Find the branch name associated with a worktree path
get_branch_from_worktree_path() {
    local WORKTREE_PATH=$1
    # Use porcelain format to reliably parse
    git worktree list --porcelain | grep "^worktree $WORKTREE_PATH" -A 1 | grep "^branch " | sed 's|^branch refs/heads/||'
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
    # --- [FIX #1: Added 2>/dev/null to silence harmless 'fatal: branch...exists' error] ---
    if ! git worktree add -b "$BRANCH_NAME" "$WORKTREE_PATH" 2>/dev/null; then
        echo "Branch '$BRANCH_NAME' already exists. Creating worktree from existing branch."
        git worktree add "$WORKTREE_PATH" "$BRANCH_NAME"
    fi
    
    # --- [FIX #2: Added -c protocol.file.allow=always] ---
    # This tells Git to allow the 'file://' transport just for this one command.
    echo "→ Initializing submodules in new worktree..."
    (
        cd "$WORKTREE_PATH"
        for sub in ${SUBMODULES[@]}; do
            url=$(git config -f .gitmodules submodule.$sub.url)
            git clone --reference "$REPO_ROOT" "$url" "$sub"
        done
    )
    echo "  ✓ Submodules initialized."
    # --- [END FIXES] ---
    
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
            "name": "${WORKTREE_NAME} (Workspace)",
            "path": "."
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

    # --- [NEW LOGIC] ---
    # Reliably get the branch name from the worktree path
    local FEATURE_BRANCH=$(get_branch_from_worktree_path "$WORKTREE_PATH")
    
    # Check if the main feature branch has been merged
    echo "Checking if worktree branch '$FEATURE_BRANCH' has been merged..."
    local UNMERGED_BRANCHES=()

    if [ -n "$FEATURE_BRANCH" ]; then
        # Find the default branch (main or master)
        local MAIN_BRANCH
        MAIN_BRANCH=$(git symbolic-ref refs/remotes/origin/HEAD 2>/dev/null | sed 's@^refs/remotes/origin/@@' || echo "main")
        
        if ! git branch --merged "$MAIN_BRANCH" | grep -q "^\*\?[[:space:]]*$FEATURE_BRANCH$"; then
            UNMERGED_BRANCHES+=("superproject: $FEATURE_BRANCH")
        fi
    else
        echo "Warning: Could not determine branch for worktree $WORKTREE_NAME."
    fi

    # Warn if unmerged branches found
    if [ ${#UNMERGED_BRANCHES[@]} -gt 0 ]; then
        echo "⚠ WARNING: The following branch has NOT been merged into $MAIN_BRANCH:"
        for branch in "${UNMERGED_BRANCHES[@]}"; do
            echo "  • $branch"
        done
        echo ""
        read -p "Are you sure you want to remove this worktree? (y/N) " -n 1 -r
        echo ""
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            echo "Removal cancelled"
            exit 0
        fi
    else
        echo "✓ Branch '$FEATURE_BRANCH' appears to be merged."
    fi

    echo "Removing worktree: $WORKTREE_NAME"
    
    # Remove main worktree (this is all that's needed)
    echo "→ Removing main worktree"
    git worktree remove "$WORKTREE_PATH" --force
    
    # The branch is left behind by default, which is safer.
    # We can ask to delete it.
    if [ -n "$FEATURE_BRANCH" ] && [ ${#UNMERGED_BRANCHES[@]} -eq 0 ]; then
        read -p "Do you want to delete the branch '$FEATURE_BRANCH' as well? (y/N) " -n 1 -r
        echo ""
        if [[ $REPLY =~ ^[Yy]$ ]]; then
            git branch -d "$FEATURE_BRANCH"
            echo "✓ Branch '$FEATURE_BRANCH' deleted."
        fi
    fi
    
    echo "✓ Worktree removed successfully"
    # --- [END NEW LOGIC] ---
}

merge_worktree() {
    local WORKTREE_NAME=$1
    local TARGET_BRANCH=${2:-main}
    local WORKTREE_PATH="$WORKTREE_DIR/$WORKTREE_NAME"

    if [ ! -d "$WORKTREE_PATH" ]; then
        echo "Error: Worktree $WORKTREE_NAME not found at $WORKTREE_PATH"
        exit 1
    fi

    local ORIGINAL_DIR=$(pwd)
    cd "$WORKTREE_PATH"
    local FEATURE_BRANCH=$(git branch --show-current)
    cd "$ORIGINAL_DIR"

    if [ -z "$FEATURE_BRANCH" ]; then
        echo "Error: Could not determine branch for worktree $WORKTREE_NAME."
        exit 1
    fi

    echo "Merging branch '$FEATURE_BRANCH' (from '$WORKTREE_NAME') into '$TARGET_BRANCH'"
    echo "================================================"
    echo ""

    local ORIGINAL_BRANCH=$(git branch --show-current)

    if [ -n "$(git status --porcelain)" ]; then
        echo "Error: You have uncommitted changes in the main repository."
        echo "Please commit or stash them before merging."
        exit 1
    fi
    
    echo "→ Checking out '$TARGET_BRANCH' in main repo"
    git checkout "$TARGET_BRANCH"

    echo "→ Merging '$FEATURE_BRANCH' into '$TARGET_BRANCH'"
    if ! git merge "$FEATURE_BRANCH" --no-edit; then
        echo "  ✗ MERGE CONFLICT DETECTED!"
        echo "  Please resolve conflicts in the main repo, then run:"
        echo "    git merge --continue"
        echo "  After resolving, you must manually run:"
        echo "    git -c protocol.file.allow=always submodule update --init --recursive"
        git checkout "$ORIGINAL_BRANCH" > /dev/null 2>&1 || true
        exit 1
    fi
    echo "  ✓ Merge successful"
    
    echo "→ Syncing submodules to their new merged state..."
    # --- [FIXED LINE] ---
    if ! git submodule foreach 'git -c protocol.file.allow=always pull origin main'; then
    # --- [END FIX] ---
        echo "  ✗ FAILED to update submodules."
        echo "  This can happen if a submodule commit was not pushed."
        echo "  Please check the merge and run 'git -c protocol.file.allow=always submodule update' manually."
        git checkout "$ORIGINAL_BRANCH" > /dev/null 2>&1 || true
        exit 1
    fi
    echo "  ✓ Submodules synced."
    
    git checkout "$ORIGINAL_BRANCH" > /dev/null 2>&1 || true

    echo ""
    echo "================================================"
    echo "✓ Merge complete!"
    echo "'$TARGET_BRANCH' is now updated with changes from '$FEATURE_BRANCH'."
}

sync_worktree() {
    local WORKTREE_NAME=$1
    local TARGET_BRANCH=${2:-main}
    local WORKTREE_PATH="$WORKTREE_DIR/$WORKTREE_NAME"

    if [ ! -d "$WORKTREE_PATH" ]; then
        echo "Error: Worktree $WORKTREE_NAME not found at $WORKTREE_PATH"
        exit 1
    fi

    echo "Syncing worktree '$WORKTREE_NAME' with '$TARGET_BRANCH'"
    echo "================================================"
    
    # --- [NEW LOGIC] ---
    (
        # Run all sync operations from inside the worktree
        cd "$WORKTREE_PATH"
        
        local FEATURE_BRANCH=$(git branch --show-current)
        echo "→ Merging '$TARGET_BRANCH' into '$FEATURE_BRANCH' (in $WORKTREE_NAME)..."

        # Check for uncommitted changes in worktree
        if [ -n "$(git status --porcelain)" ]; then
            echo "  Error: Uncommitted changes in worktree: $WORKTREE_NAME"
            echo "  Please commit or stash changes before syncing."
            exit 1
        fi

        # Check if target branch exists
        if ! git rev-parse --verify "$TARGET_BRANCH" >/dev/null 2>&1; then
            echo "  ✗ Target branch '$TARGET_BRANCH' does not exist"
            exit 1
        fi

        # 1. Merge the target branch into the worktree's branch
        if ! git merge "$TARGET_BRANCH" --no-edit; then
            echo "  ✗ MERGE CONFLICT DETECTED!"
            echo "  Please resolve conflicts in $WORKTREE_PATH"
            echo "  After resolving, you must manually run:"
            echo "    git submodule update --init --recursive"
            exit 1
        fi
        echo "  ✓ Main branch synced."
        
        # 2. Update submodules to pull in any new ones from the merge
        echo "→ Updating submodules in worktree..."
        if ! git submodule foreach 'git -c protocol.file.allow=always pull origin main'; then
             echo "  ✗ FAILED to update submodules in worktree."
             exit 1
        fi
        echo "  ✓ Submodules synced."
    )
    
    echo ""
    echo "================================================"
    echo "✓ Sync complete!"
    echo "Worktree '$WORKTREE_NAME' is now up to date with '$TARGET_BRANCH'"
    # --- [END NEW LOGIC] ---
}

pull_from_worktree() {
    local TARGET_BRANCH=${1:-main}

    # This command pulls from remote into the *current* directory (worktree or main)
    echo "Pulling latest changes from origin/$TARGET_BRANCH into current directory"
    echo "================================================"

    # --- [NEW LOGIC] ---
    # Check for uncommitted changes
    if [ -n "$(git status --porcelain)" ]; then
        echo "  Error: Uncommitted changes detected"
        echo "  Please commit or stash changes before pulling"
        exit 1
    fi

    echo "→ Fetching from remote"
    git fetch origin
    
    echo "→ Merging 'origin/$TARGET_BRANCH' into current branch"
    if ! git merge "origin/$TARGET_BRANCH" --no-edit; then
        echo "  ✗ MERGE CONFLICT DETECTED!"
        echo "  Please resolve conflicts in current directory."
        echo "  After resolving, you must manually run:"
        echo "    git submodule update --init --recursive"
        exit 1
    fi
    echo "  ✓ Main branch pulled."
    
    echo "→ Updating submodules to match pulled state..."
    if ! git submodule foreach 'git -c protocol.file.allow=always pull origin main'; then
        echo "  ✗ FAILED to update submodules."
        exit 1
    fi
    echo "  ✓ Submodules synced."
    
    echo ""
    echo "================================================"
    echo "✓ Pull complete!"
    # --- [END NEW LOGIC] ---
}

push_worktree() {
    # This command pushes the TARGET_BRANCH (e.g., 'main') to origin
    # It correctly pushes submodules first, then the superproject.
    # This logic was already correct and did not need changing.
    
    local TARGET_BRANCH=${1:-main}

    echo "Pushing '$TARGET_BRANCH' branches to origin"
    echo "================================================"
    
    # Must be run from the main repo folder
    cd "$REPO_ROOT"
    
    # Ensure we are on the target branch
    local CURRENT_BRANCH=$(git branch --show-current)
    if [ "$CURRENT_BRANCH" != "$TARGET_BRANCH" ]; then
        echo "Error: You are on branch '$CURRENT_BRANCH', not '$TARGET_BRANCH'."
        echo "Please 'git checkout $TARGET_BRANCH' in your main repo first."
        exit 1
    fi
    echo "→ On branch '$TARGET_BRANCH' in main repo."
    
    # Push each submodule
    echo "→ Pushing submodules..."
    # Use 'git submodule foreach' for the correct logic
    if ! git submodule foreach "git push origin $TARGET_BRANCH"; then
        echo "  ✗ FAILED to push one or more submodules."
        echo "  Please check errors above. Aborting superproject push."
        exit 1
    fi
    echo "  ✓ All submodules pushed."

    # Push main branch
    echo "→ Pushing main repository"
    if git push origin "$TARGET_BRANCH"; then
        echo "  ✓ Push successful"
    else
        echo "  ✗ Push failed"
        exit 1
    fi

    echo ""
    echo "================================================"
    echo "✓ Push complete!"
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
                
                # --- [THIS BLOCK IS FIXED] ---
                if [ -f "$workspace" ]; then
                    echo "  • $name (workspace: $workspace)"
                else
                    echo "  • $name (no .code-workspace file found)"
                fi
                # --- [END FIX] ---
                
            fi
        done
    fi
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
    merge)
        if [ $# -lt 1 ]; then
            show_usage
        fi
        merge_worktree "$@"
        ;;
    sync)
        if [ $# -lt 1 ]; then
            show_usage
        fi
        sync_worktree "$@"
        ;;
    pull)
        pull_from_worktree "$@"
        ;;
    push)
        if [ $# -lt 1 ]; then
            # Default to 'main' if no branch is specified
            push_worktree "main"
        else
            push_worktree "$@"
        fi
        ;;
    list)
        list_worktrees
        ;;
    complete)
        # Generic completion API - called by shell completion scripts
        case $# in
            0)
                # First argument (command names)
                echo "create remove open merge sync pull push list setup"
                ;;
            1)
                # Second argument (worktree names)
                case "$1" in
                    remove|open|merge|sync)
                        if [ -d "$WORKTREE_DIR" ]; then
                            ls -1 "$WORKTREE_DIR" 2>/dev/null | tr '\n' ' '
                        fi
                        ;;
                    *)
                        ;;
                esac
                ;;
            *)
                ;;
        esac
        ;;
    setup)
        # This assumes your completion files are in a .worktree-manager dir
        # This logic is complex and specific to your setup, so I am leaving it as-is.
        DETECTED_SHELL=""
        if command -v ps >/dev/null 2>&1; then
            PARENT_PID=$PPID
            PARENT_PROC=$(ps -p $PARENT_PID -o comm= 2>/dev/null | tr -d ' ')
            if [[ "$PARENT_PROC" == *"zsh"* ]]; then
                DETECTED_SHELL="zsh"
            elif [[ "$PARENT_PROC" == *"bash"* ]]; then
                DETECTED_SHELL="bash"
            fi
        fi

        SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
        COMPLETION_DIR="$SCRIPT_DIR/.worktree-manager"

        if [ "$DETECTED_SHELL" = "zsh" ]; then
            COMPLETION_FILE="$COMPLETION_DIR/completion.zsh"
            if [ -f "$COMPLETION_FILE" ]; then
                cat "$COMPLETION_FILE"
                echo "echo '✓ Zsh tab completion enabled for current shell session'"
            else
                echo "echo 'Error: Completion file not found: $COMPLETION_FILE' >&2" >&2
                exit 1
            fi
        elif [ "$DETECTED_SHELL" = "bash" ]; then
            COMPLETION_FILE="$COMPLETION_DIR/completion.bash"
            if [ -f "$COMPLETION_FILE" ]; then
                cat "$COMPLETION_FILE"
                echo "echo '✓ Bash tab completion enabled for current shell session'"
            else
                echo "echo 'Error: Completion file not found: $COMPLETION_FILE' >&2" >&2
                exit 1
            fi
        else
            echo "echo 'Error: Unsupported shell. Please use bash or zsh.' >&2" >&2
            exit 1
        fi
        ;;
    *)
        echo "Error: Unknown command '$COMMAND'"
        show_usage
        ;;
esac