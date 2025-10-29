 - Clean up commands/command_macros/src/command.rs - all the traits that get quoted in should be easily implementable as a regular trait that is used ther or ar egular templateize dclass. The logic is still too much in the macro
 - Lots of the handling of completions should not be happening in tui/src/main.rs
  - Compeletion trait/impl should do that, more-over, it should be asy to wrap a completer with a prefix-caching completer.

