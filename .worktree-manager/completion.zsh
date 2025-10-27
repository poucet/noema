# Zsh completion for worktree-manager.sh
# This file should be sourced to enable completion

autoload -Uz compinit && compinit
autoload -Uz bashcompinit && bashcompinit

_worktree_manager_zsh() {
    local -a commands worktrees
    local script_path

    # Find the script path from the command being completed
    script_path="${words[1]}"

    if (( CURRENT == 2 )); then
        # Complete commands - get list from the script
        local cmd_list=$("$script_path" complete commands 2>/dev/null)
        # Convert space-separated list to array with descriptions
        local -a cmd_array
        for cmd in ${(z)cmd_list}; do
            case "$cmd" in
                create) cmd_array+=("create:Create a new worktree with submodules and open in VSCode") ;;
                remove) cmd_array+=("remove:Remove a worktree and its submodules (checks if merged)") ;;
                open) cmd_array+=("open:Open an existing worktree in VSCode") ;;
                merge) cmd_array+=("merge:Merge worktree branches back into target branch") ;;
                push) cmd_array+=("push:Push target branch to origin") ;;
                list) cmd_array+=("list:List all worktrees") ;;
                setup) cmd_array+=("setup:Enable tab-completion for current shell session") ;;
                *) cmd_array+=("$cmd") ;;
            esac
        done
        _describe 'command' cmd_array
    elif (( CURRENT == 3 )); then
        case "$words[2]" in
            remove|open|merge|push)
                # Complete worktree names - get list from the script
                local wt_list=$("$script_path" complete worktrees 2>/dev/null)
                worktrees=(${(z)wt_list})
                _describe 'worktree' worktrees
                ;;
        esac
    fi
}

compdef _worktree_manager_zsh worktree-manager.sh
compdef _worktree_manager_zsh ./worktree-manager.sh
