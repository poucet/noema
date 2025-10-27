# Generic zsh completion that works with any command implementing the 'complete' API
# This file should be sourced to enable completion
#
# Required API:
#   <command> complete [words...]
#
# The command receives all typed words (excluding the command itself)
# and returns space-separated completion options.

autoload -Uz compinit && compinit
autoload -Uz bashcompinit && bashcompinit

_worktree_manager_zsh() {
    local script_path result
    local -a completions prev_words

    # Find the script path from the command being completed
    script_path="${words[1]}"

    # Collect words typed so far (excluding command name and current incomplete word)
    # In zsh, words[2] is first arg, words[3] is second arg, etc.
    # CURRENT is the index of word being completed
    if (( CURRENT > 2 )); then
        prev_words=("${words[@]:2:$((CURRENT-2))}")
    fi

    # Get completions from the script
    result=$("$script_path" complete "${prev_words[@]}" 2>/dev/null)

    if [ -n "$result" ]; then
        completions=(${(z)result})
        _describe 'option' completions
    fi
}

compdef _worktree_manager_zsh worktree-manager.sh
compdef _worktree_manager_zsh ./worktree-manager.sh
