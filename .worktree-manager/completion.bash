# Bash completion for worktree-manager.sh
# This file should be sourced to enable completion

_worktree_manager_completion() {
    local cur prev script_path
    COMPREPLY=()
    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD-1]}"

    # Find the script path (could be ./worktree-manager.sh or worktree-manager.sh)
    script_path="${COMP_WORDS[0]}"

    # Complete command names
    if [ $COMP_CWORD -eq 1 ]; then
        local commands=$("$script_path" complete commands 2>/dev/null)
        COMPREPLY=($(compgen -W "$commands" -- "$cur"))
        return 0
    fi

    # Complete worktree names for commands that need them
    if [ $COMP_CWORD -eq 2 ]; then
        case "$prev" in
            remove|open|merge|push)
                local worktrees=$("$script_path" complete worktrees 2>/dev/null)
                COMPREPLY=($(compgen -W "$worktrees" -- "$cur"))
                return 0
                ;;
        esac
    fi

    return 0
}

complete -F _worktree_manager_completion ./worktree-manager.sh
complete -F _worktree_manager_completion worktree-manager.sh
