# Generic bash completion that works with any command implementing the 'complete' API
# This file should be sourced to enable completion
#
# Required API:
#   <command> complete [words...]
#
# The command receives all typed words (excluding the command itself)
# and returns space-separated completion options.

_worktree_manager_completion() {
    local cur script_path completions
    COMPREPLY=()
    cur="${COMP_WORDS[COMP_CWORD]}"
    script_path="${COMP_WORDS[0]}"

    # Pass all words typed so far (excluding the script name and current incomplete word)
    completions=$("$script_path" complete "${COMP_WORDS[@]:1:$((COMP_CWORD-1))}" 2>/dev/null)

    COMPREPLY=($(compgen -W "$completions" -- "$cur"))
    return 0
}

complete -F _worktree_manager_completion ./worktree-manager.sh
complete -F _worktree_manager_completion worktree-manager.sh
