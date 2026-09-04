; Capture names shared by every language's query, so one reader in
; `code` drives them all: `@import.source` is what a `use` names,
; `@module.decl` a `mod x;` that pulls in another file.

(use_declaration
  argument: (_) @import.source)

(mod_item
  !body
  name: (identifier) @module.decl)
