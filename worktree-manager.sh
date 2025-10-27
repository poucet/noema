#!/bin/bash

# Git Worktree Manager for repositories with submodules
# Usage: ./worktree-manager.sh <command> <worktree-name>

set -e

# Get the absolute path to the repository root
REPO_ROOT="$(git rev-parse --show-toplevel)"
WORKTREE_DIR="$(dirname "$REPO_ROOT")/athena-worktrees"

# Dynamically discover submodules
get_submodules() {
    git config --file .gitmodules --get-regexp path | awk '{print $2}'
}

# Cache submodules for the script execution
SUBMODULES=($(get_submodules))

show_usage() {
    cat << EOF
Usage: $0 <command> <worktree-name> [branch-name|target-branch]

Commands:
  create    Create a new worktree with submodule worktrees and open in VSCode
  remove    Remove a worktree and its submodule worktrees (checks if merged)
  open      Open an existing worktree in VSCode
  merge     Merge worktree branches back into target branch (default: main)
  sync      Sync worktree with target branch - pull latest changes (default: main)
  push      Push target branch to origin (default: main)
  list      List all worktrees
  setup     Enable tab-completion for current shell session
  complete  Internal command for shell completion (use: complete commands|worktrees)

Examples:
  $0 create feature-1                    # Create from current branch and open in VSCode
  $0 create feature-1 my-branch          # Create from specific branch and open in VSCode
  $0 open feature-1                      # Open existing worktree in VSCode
  $0 merge feature-1                     # Merge feature-1 branches into main
  $0 merge feature-1 develop             # Merge feature-1 branches into develop
  $0 sync feature-1                      # Sync feature-1 worktree with latest main
  $0 sync feature-1 develop              # Sync feature-1 worktree with latest develop
  $0 push feature-1                      # Push main branch to origin
  $0 push feature-1 develop              # Push develop branch to origin
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
            "name": "athena",
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
    local FEATURE_BRANCH=$WORKTREE_NAME

    if [ ! -d "$WORKTREE_PATH" ]; then
        echo "Error: Worktree $WORKTREE_NAME not found at $WORKTREE_PATH"
        exit 1
    fi

    # Check if branches have been merged
    echo "Checking if worktree branches have been merged..."
    local UNMERGED_BRANCHES=()

    # Check main feature branch
    if git rev-parse --verify "$FEATURE_BRANCH" >/dev/null 2>&1; then
        local MAIN_BRANCH=$(git symbolic-ref refs/remotes/origin/HEAD 2>/dev/null | sed 's@^refs/remotes/origin/@@' || echo "main")
        if ! git branch --merged "$MAIN_BRANCH" | grep -q "^\*\?[[:space:]]*$FEATURE_BRANCH$"; then
            UNMERGED_BRANCHES+=("main repo: $FEATURE_BRANCH")
        fi
    fi

    # Check submodule branches
    for submodule in "${SUBMODULES[@]}"; do
        if [ -d "$submodule/.git" ] || [ -f "$submodule/.git" ]; then
            cd "$submodule"
            local SUB_BRANCH="$FEATURE_BRANCH-$submodule"
            if git rev-parse --verify "$SUB_BRANCH" >/dev/null 2>&1; then
                local SUB_MAIN=$(git symbolic-ref refs/remotes/origin/HEAD 2>/dev/null | sed 's@^refs/remotes/origin/@@' || echo "main")
                if ! git branch --merged "$SUB_MAIN" | grep -q "^\*\?[[:space:]]*$SUB_BRANCH$"; then
                    UNMERGED_BRANCHES+=("$submodule: $SUB_BRANCH")
                fi
            fi
            cd - > /dev/null
        fi
    done

    # Warn if unmerged branches found
    if [ ${#UNMERGED_BRANCHES[@]} -gt 0 ]; then
        echo "⚠ WARNING: The following branches have NOT been merged:"
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
        echo "✓ All branches have been merged"
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

    # Discover submodules from BOTH main and the worktree to handle new submodules
    echo "→ Discovering submodules from worktree and main..."
    local MAIN_SUBMODULES=($(get_submodules))
    local WORKTREE_SUBMODULES=($(cd "$WORKTREE_PATH" && git config --file .gitmodules --get-regexp path 2>/dev/null | awk '{print $2}'))

    # Combine and deduplicate submodules
    local ALL_SUBMODULES=($(printf '%s\n' "${MAIN_SUBMODULES[@]}" "${WORKTREE_SUBMODULES[@]}" | sort -u))

    if [ ${#WORKTREE_SUBMODULES[@]} -gt ${#MAIN_SUBMODULES[@]} ]; then
        echo "  ℹ Detected new submodules in worktree that don't exist in main yet"
    fi
    echo ""

    # Merge each submodule
    for submodule in "${ALL_SUBMODULES[@]}"; do
        local SUB_BRANCH="$FEATURE_BRANCH-$submodule"

        # Check if submodule exists in main OR in worktree
        local SUBMODULE_EXISTS=false
        if [ -d "$submodule/.git" ] || [ -f "$submodule/.git" ]; then
            SUBMODULE_EXISTS=true
        elif [ -d "$WORKTREE_PATH/$submodule/.git" ] || [ -f "$WORKTREE_PATH/$submodule/.git" ]; then
            SUBMODULE_EXISTS=true
            echo "→ New submodule detected: $submodule (exists in worktree but not in main)"
        fi

        if [ "$SUBMODULE_EXISTS" = false ]; then
            echo "  ⚠ Submodule '$submodule' not initialized in either location, skipping"
            continue
        fi

        # Determine the submodule git directory (could be in main or worktree)
        local SUBMODULE_GIT_DIR
        if [ -d "$submodule/.git" ] || [ -f "$submodule/.git" ]; then
            SUBMODULE_GIT_DIR="$submodule"
        else
            SUBMODULE_GIT_DIR="$WORKTREE_PATH/$submodule"
        fi

        echo "→ Processing submodule: $submodule"
        cd "$SUBMODULE_GIT_DIR"

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
            echo "    cd $SUBMODULE_GIT_DIR && git merge --continue && cd .."
            exit 1
        fi

        cd - > /dev/null
        echo ""
    done
    
    # Update submodule references in main repo
    echo "→ Updating submodule references in main repository"

    # Add all submodules (both existing and new ones)
    for submodule in "${ALL_SUBMODULES[@]}"; do
        # Only add if the submodule path exists (it might be new and need initialization first)
        if [ -e "$submodule" ]; then
            git add "$submodule" 2>/dev/null || true
        fi
    done

    # Also add .gitmodules in case new submodules were added
    git add .gitmodules 2>/dev/null || true

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

    # Initialize any new submodules that were added in the worktree
    if [ ${#WORKTREE_SUBMODULES[@]} -gt ${#MAIN_SUBMODULES[@]} ]; then
        echo ""
        echo "→ Initializing new submodules in main worktree"

        # For each new submodule, we need to properly initialize it
        for submodule in "${ALL_SUBMODULES[@]}"; do
            # Check if this is a new submodule (exists in worktree but wasn't in main)
            local IS_NEW=true
            for existing in "${MAIN_SUBMODULES[@]}"; do
                if [ "$submodule" = "$existing" ]; then
                    IS_NEW=false
                    break
                fi
            done

            if [ "$IS_NEW" = true ]; then
                echo "  • Initializing new submodule: $submodule"

                local WORKTREE_SUBMODULE_PATH="$WORKTREE_PATH/$submodule"
                local SUB_BRANCH="$FEATURE_BRANCH-$submodule"

                # Check if the submodule exists in the worktree
                if [ ! -d "$WORKTREE_SUBMODULE_PATH/.git" ] && [ ! -f "$WORKTREE_SUBMODULE_PATH/.git" ]; then
                    echo "    ⚠ Submodule not found in worktree at $WORKTREE_SUBMODULE_PATH"
                    continue
                fi

                # Get the submodule URL from .gitmodules
                local SUBMODULE_URL=$(git config --file .gitmodules --get "submodule.$submodule.url")

                # If URL is relative (starts with ./ or ../), we need to set up the submodule properly
                if [[ "$SUBMODULE_URL" == ./* ]] || [[ "$SUBMODULE_URL" == ../* ]]; then
                    echo "    • Setting up repository from relative URL: $SUBMODULE_URL"

                    # Step 1: Find the actual git repository in the worktree (handle worktree pointers)
                    local WORKTREE_SUB_GIT_DIR
                    if [ -f "$WORKTREE_SUBMODULE_PATH/.git" ]; then
                        # It's a worktree pointer - find the actual git directory
                        WORKTREE_SUB_GIT_DIR=$(cat "$WORKTREE_SUBMODULE_PATH/.git" | sed 's/^gitdir: //')
                        # Make it absolute if it's relative
                        if [[ "$WORKTREE_SUB_GIT_DIR" != /* ]]; then
                            WORKTREE_SUB_GIT_DIR="$WORKTREE_SUBMODULE_PATH/$WORKTREE_SUB_GIT_DIR"
                        fi
                        # Get the main git directory (parent of worktrees)
                        WORKTREE_SUB_GIT_DIR=$(dirname "$(dirname "$WORKTREE_SUB_GIT_DIR")")
                    elif [ -d "$WORKTREE_SUBMODULE_PATH/.git" ]; then
                        # Standalone git directory
                        WORKTREE_SUB_GIT_DIR="$WORKTREE_SUBMODULE_PATH"
                    else
                        echo "    ⚠ Cannot find git directory for $submodule"
                        continue
                    fi

                    echo "      Found source git repo at: $WORKTREE_SUB_GIT_DIR"

                    # Check for uncommitted changes in the worktree submodule
                    echo "      Checking for uncommitted changes in worktree submodule"
                    cd "$WORKTREE_SUBMODULE_PATH"
                    local HAS_CHANGES=false
                    if [ -n "$(git status --porcelain)" ]; then
                        HAS_CHANGES=true
                        echo "      ⚠ Found uncommitted changes - will preserve them"
                    fi
                    cd - > /dev/null

                    # Step 2: Clone the repository as a standalone repo in main
                    echo "      Cloning as independent repository to main"
                    if git clone "$WORKTREE_SUB_GIT_DIR" "$submodule"; then
                        cd "$submodule"

                        # Remove the origin remote (we don't want it pointing to worktree)
                        echo "      Removing origin remote"
                        git remote remove origin 2>/dev/null || true

                        # Checkout the target branch if it exists
                        if git rev-parse --verify "$TARGET_BRANCH" >/dev/null 2>&1; then
                            git checkout "$TARGET_BRANCH"
                            echo "      Checked out $TARGET_BRANCH"
                        else
                            echo "      Staying on current branch"
                        fi

                        cd - > /dev/null
                        echo "    ✓ Created independent repo at $submodule"

                        # Step 3: Convert the worktree submodule to be a worktree of the new main submodule
                        echo "    • Converting worktree submodule to worktree of main submodule"

                        # Save the working directory state if there are uncommitted changes
                        local TEMP_STASH_DIR=""
                        if [ "$HAS_CHANGES" = true ]; then
                            echo "      Backing up uncommitted changes"
                            TEMP_STASH_DIR=$(mktemp -d)
                            rsync -a --exclude='.git' "$WORKTREE_SUBMODULE_PATH/" "$TEMP_STASH_DIR/"
                        fi

                        cd "$submodule"

                        # Remove the worktree submodule from the worktree's git tracking
                        # (it's currently either a standalone repo or worktree of worktree's parent)
                        if [ -f "$WORKTREE_SUBMODULE_PATH/.git" ]; then
                            # It's a worktree - need to remove it from its parent's worktree list
                            local OLD_PARENT_GIT_DIR=$(dirname "$(dirname "$WORKTREE_SUB_GIT_DIR")")
                            if [ -d "$OLD_PARENT_GIT_DIR" ]; then
                                cd "$OLD_PARENT_GIT_DIR"
                                git worktree remove "$WORKTREE_SUBMODULE_PATH" --force 2>/dev/null || true
                                cd - > /dev/null
                            fi
                        fi

                        # Now add it as a worktree of the main submodule
                        cd "$REPO_ROOT/$submodule"
                        echo "      Adding worktree at $WORKTREE_SUBMODULE_PATH with branch $SUB_BRANCH"

                        # Check if the branch exists
                        if git rev-parse --verify "$SUB_BRANCH" >/dev/null 2>&1; then
                            git worktree add "$WORKTREE_SUBMODULE_PATH" "$SUB_BRANCH" --force 2>/dev/null || true
                        else
                            git worktree add -b "$SUB_BRANCH" "$WORKTREE_SUBMODULE_PATH" --force 2>/dev/null || true
                        fi

                        # Restore uncommitted changes if we backed them up
                        if [ "$HAS_CHANGES" = true ] && [ -n "$TEMP_STASH_DIR" ]; then
                            echo "      Restoring uncommitted changes"
                            rsync -a --exclude='.git' "$TEMP_STASH_DIR/" "$WORKTREE_SUBMODULE_PATH/"
                            rm -rf "$TEMP_STASH_DIR"
                            echo "      ✓ Uncommitted changes preserved"
                        fi

                        cd - > /dev/null
                        echo "    ✓ Worktree submodule now points to main submodule"
                    else
                        echo "    ⚠ Failed to clone $submodule"
                        continue
                    fi
                else
                    # URL is absolute, can use normal git clone
                    echo "    • Cloning from remote: $SUBMODULE_URL"
                    if git clone "$SUBMODULE_URL" "$submodule"; then
                        cd "$submodule"
                        git checkout "$TARGET_BRANCH" 2>/dev/null || echo "    (no $TARGET_BRANCH branch in $submodule yet)"
                        cd - > /dev/null
                        echo "    ✓ Cloned $submodule"
                    else
                        echo "    ⚠ Failed to clone $submodule"
                    fi
                fi
            fi
        done
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
    echo ""

    # Note: Worktrees share the same git repository, so we don't need to pull.
    # We just need to make sure the target branch exists and is up to date.
    # The user should pull in their main worktree before running sync.

    # Discover submodules from the worktree
    local WORKTREE_SUBMODULES=($(cd "$WORKTREE_PATH" && git config --file .gitmodules --get-regexp path 2>/dev/null | awk '{print $2}'))

    # Sync the worktree with the target branch
    echo "→ Syncing worktree main branch with $TARGET_BRANCH"
    cd "$WORKTREE_PATH"

    # Check for uncommitted changes in worktree
    if [ -n "$(git status --porcelain)" ]; then
        echo "  ⚠ Warning: Uncommitted changes in worktree"
        echo "  Please commit or stash changes before syncing"
        cd - > /dev/null
        exit 1
    fi

    # Check if target branch exists
    if ! git rev-parse --verify "$TARGET_BRANCH" >/dev/null 2>&1; then
        echo "  ✗ Target branch '$TARGET_BRANCH' does not exist"
        cd - > /dev/null
        exit 1
    fi

    local FEATURE_BRANCH=$(git branch --show-current)
    echo "  • Merging $TARGET_BRANCH into $FEATURE_BRANCH"

    if git merge "$TARGET_BRANCH" --no-edit; then
        echo "  ✓ Worktree main branch synced"
    else
        echo "  ✗ Merge conflict detected in worktree!"
        echo "  Please resolve conflicts in $WORKTREE_PATH"
        cd - > /dev/null
        exit 1
    fi

    cd - > /dev/null
    echo ""

    # Sync each submodule worktree
    for submodule in "${WORKTREE_SUBMODULES[@]}"; do
        local SUB_PATH="$WORKTREE_PATH/$submodule"

        if [ -d "$SUB_PATH/.git" ] || [ -f "$SUB_PATH/.git" ]; then
            echo "→ Syncing submodule worktree: $submodule"
            cd "$SUB_PATH"

            # Check for uncommitted changes
            if [ -n "$(git status --porcelain)" ]; then
                echo "  ⚠ Warning: Uncommitted changes in $submodule worktree"
                echo "  Skipping this submodule. Please commit or stash changes first."
                cd - > /dev/null
                continue
            fi

            # Check if target branch exists
            if ! git rev-parse --verify "$TARGET_BRANCH" >/dev/null 2>&1; then
                echo "  ⚠ Target branch '$TARGET_BRANCH' does not exist in $submodule"
                cd - > /dev/null
                continue
            fi

            local SUB_FEATURE_BRANCH=$(git branch --show-current)
            echo "  • Merging $TARGET_BRANCH into $SUB_FEATURE_BRANCH"

            if git merge "$TARGET_BRANCH" --no-edit; then
                echo "  ✓ Submodule worktree synced"
            else
                echo "  ✗ Merge conflict detected in $submodule worktree!"
                echo "  Please resolve conflicts in $SUB_PATH"
                cd - > /dev/null
                exit 1
            fi

            cd - > /dev/null
            echo ""
        fi
    done

    echo "================================================"
    echo "✓ Sync complete!"
    echo ""
    echo "Worktree '$WORKTREE_NAME' is now up to date with '$TARGET_BRANCH'"
    echo ""
    echo "Note: This merges the current state of '$TARGET_BRANCH' into your worktree."
    echo "If you need the latest changes from remote, first run 'git pull' in your"
    echo "main worktree to update '$TARGET_BRANCH', then run this sync command."
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
    sync)
        if [ $# -lt 1 ]; then
            show_usage
        fi
        sync_worktree "$@"
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
    complete)
        # Generic completion API - called by shell completion scripts
        # Usage: complete [words...]
        # Returns space-separated completion options based on the number of words passed

        case $# in
            0)
                # No words yet - complete first argument (command names)
                echo "create remove open merge sync push list setup"
                ;;
            1)
                # One word (the command) - complete second argument based on command
                case "$1" in
                    remove|open|merge|sync|push)
                        # These commands take worktree names as second argument
                        if [ -d "$WORKTREE_DIR" ]; then
                            ls -1 "$WORKTREE_DIR" 2>/dev/null | tr '\n' ' '
                        fi
                        ;;
                    *)
                        # Other commands don't take a second argument, or it's free-form
                        ;;
                esac
                ;;
            *)
                # More words - no completion for additional positions
                ;;
        esac
        ;;
    setup)
        # Output source command for the appropriate completion file
        # Detect shell type
        DETECTED_SHELL=""

        # Try to detect from parent process
        if command -v ps >/dev/null 2>&1; then
            PARENT_PID=$PPID
            PARENT_PROC=$(ps -p $PARENT_PID -o comm= 2>/dev/null | tr -d ' ')
            if [[ "$PARENT_PROC" == *"zsh"* ]]; then
                DETECTED_SHELL="zsh"
            elif [[ "$PARENT_PROC" == *"bash"* ]]; then
                DETECTED_SHELL="bash"
            fi
        fi

        # Get the directory where this script is located
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