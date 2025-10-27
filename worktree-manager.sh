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
Usage: $0 <command> <worktree-name> [branch-name|target-branch]

Commands:
  create    Create a new worktree with submodule worktrees and open in VSCode
  remove    Remove a worktree and its submodule worktrees
  open      Open an existing worktree in VSCode
  merge     Merge worktree branches back into target branch (default: main)
  push      Push target branch to origin (default: main)
  list      List all worktrees

Examples:
  $0 create feature-1                    # Create from current branch and open in VSCode
  $0 create feature-1 my-branch          # Create from specific branch and open in VSCode
  $0 open feature-1                      # Open existing worktree in VSCode
  $0 merge feature-1                     # Merge feature-1 branches into main
  $0 merge feature-1 develop             # Merge feature-1 branches into develop
  $0 push feature-1                      # Push main branch to origin
  $0 push feature-1 develop              # Push develop branch to origin
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

merge_worktree() {
    local WORKTREE_NAME=$1
    local TARGET_BRANCH=${2:-main}
    local FEATURE_BRANCH=$WORKTREE_NAME
    local WORKTREE_PATH="$WORKTREE_DIR/$WORKTREE_NAME"
    
    if [ ! -d "$WORKTREE_PATH" ]; then
        echo "Error: Worktree $WORKTREE_NAME not found at $WORKTREE_PATH"
        exit 1
    fi
    
    echo "Merging worktree '$WORKTREE_NAME' into '$TARGET_BRANCH'"
    echo "================================================"
    echo ""
    
    # Save current branch in main repo
    local ORIGINAL_BRANCH=$(git branch --show-current)
    
    # Check for uncommitted changes in main repo
    if [ -n "$(git status --porcelain)" ]; then
        echo "Error: You have uncommitted changes in the main repository."
        echo "Please commit or stash them first."
        exit 1
    fi
    
    # Merge each submodule
    for submodule in "${SUBMODULES[@]}"; do
        if [ -d "$submodule/.git" ] || [ -f "$submodule/.git" ]; then
            echo "→ Processing submodule: $submodule"
            cd "$submodule"
            
            local SUB_BRANCH="$FEATURE_BRANCH-$submodule"
            local SUB_ORIGINAL_BRANCH=$(git branch --show-current)
            
            # Check if feature branch exists
            if ! git rev-parse --verify "$SUB_BRANCH" >/dev/null 2>&1; then
                echo "  ⚠ Branch '$SUB_BRANCH' does not exist, skipping"
                cd - > /dev/null
                continue
            fi
            
            # Check for uncommitted changes
            if [ -n "$(git status --porcelain)" ]; then
                echo "  Error: Uncommitted changes in submodule $submodule"
                cd - > /dev/null
                exit 1
            fi
            
            # Checkout target branch and merge
            echo "  • Checking out $TARGET_BRANCH"
            git checkout "$TARGET_BRANCH"
            
            echo "  • Merging $SUB_BRANCH into $TARGET_BRANCH"
            if git merge "$SUB_BRANCH" --no-edit; then
                echo "  ✓ Merge successful"
            else
                echo "  ✗ Merge conflict detected!"
                echo "  Please resolve conflicts in $submodule, then run:"
                echo "    cd $submodule && git merge --continue && cd .."
                exit 1
            fi
            
            cd - > /dev/null
            echo ""
        fi
    done
    
    # Update submodule references in main repo
    echo "→ Updating submodule references in main repository"
    git add "${SUBMODULES[@]}"
    
    if [ -n "$(git status --porcelain)" ]; then
        git commit -m "Update submodules after merging $FEATURE_BRANCH"
        echo "  ✓ Submodule references updated"
    else
        echo "  • No submodule reference changes needed"
    fi
    
    # Merge main feature branch
    echo ""
    echo "→ Merging main feature branch"
    
    # Check if feature branch exists
    if ! git rev-parse --verify "$FEATURE_BRANCH" >/dev/null 2>&1; then
        echo "  ⚠ Branch '$FEATURE_BRANCH' does not exist in main repo"
    else
        echo "  • Checking out $TARGET_BRANCH"
        git checkout "$TARGET_BRANCH"
        
        echo "  • Merging $FEATURE_BRANCH into $TARGET_BRANCH"
        if git merge "$FEATURE_BRANCH" --no-edit; then
            echo "  ✓ Merge successful"
        else
            echo "  ✗ Merge conflict detected!"
            echo "  Please resolve conflicts, then run: git merge --continue"
            exit 1
        fi
    fi
    
    echo ""
    echo "================================================"
    echo "✓ Merge complete!"
    echo ""
    echo "Next steps:"
    echo "  1. Test the merged changes"
    echo "  2. Push changes to remote: $0 push $WORKTREE_NAME"
    echo "  3. If everything looks good, remove the worktree:"
    echo "     $0 remove $WORKTREE_NAME"
}

push_worktree() {
    local WORKTREE_NAME=$1
    local TARGET_BRANCH=${2:-main}
    local FEATURE_BRANCH=$WORKTREE_NAME

    echo "Pushing '$TARGET_BRANCH' branches to origin"
    echo "================================================"
    echo ""

    # Push each submodule
    for submodule in "${SUBMODULES[@]}"; do
        if [ -d "$submodule/.git" ] || [ -f "$submodule/.git" ]; then
            echo "→ Processing submodule: $submodule"
            cd "$submodule"

            local CURRENT_BRANCH=$(git branch --show-current)

            # Only push if we're on the target branch
            if [ "$CURRENT_BRANCH" = "$TARGET_BRANCH" ]; then
                echo "  • Pushing $TARGET_BRANCH to origin"
                if git push origin "$TARGET_BRANCH"; then
                    echo "  ✓ Push successful"
                else
                    echo "  ✗ Push failed"
                    cd - > /dev/null
                    exit 1
                fi
            else
                echo "  ⚠ Not on $TARGET_BRANCH (currently on $CURRENT_BRANCH), skipping"
            fi

            cd - > /dev/null
            echo ""
        fi
    done

    # Push main branch
    echo "→ Pushing main repository"
    local CURRENT_BRANCH=$(git branch --show-current)

    if [ "$CURRENT_BRANCH" = "$TARGET_BRANCH" ]; then
        echo "  • Pushing $TARGET_BRANCH to origin"
        if git push origin "$TARGET_BRANCH"; then
            echo "  ✓ Push successful"
        else
            echo "  ✗ Push failed"
            exit 1
        fi
    else
        echo "  ⚠ Not on $TARGET_BRANCH (currently on $CURRENT_BRANCH), skipping"
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
    merge)
        if [ $# -lt 1 ]; then
            show_usage
        fi
        merge_worktree "$@"
        ;;
    push)
        if [ $# -lt 1 ]; then
            show_usage
        fi
        push_worktree "$@"
        ;;
    list)
        list_worktrees
        ;;
    *)
        echo "Error: Unknown command '$COMMAND'"
        show_usage
        ;;
esac