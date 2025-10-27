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
    local -a completions

    # Find the script path from the command being completed
    script_path="${words[1]}"

    # Build array of completed words (excluding script and current word)
    # words array in zsh completion: words[1]=script, words[2]=arg1, words[3]=arg2, etc.
    # CURRENT is 1-based index of word being completed
    # Example: "./worktree-manager.sh merge <TAB>" has CURRENT=3, we need to pass "merge"
    local -a prev_words
    if (( CURRENT > 2 )); then
        # Copy words from position 2 to CURRENT-1
        prev_words=("${(@)words[2,$((CURRENT-1))]}")
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
