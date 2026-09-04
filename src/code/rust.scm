; Capture names shared by every language's query, so one reader in
; `code` drives them all: `@import.source` is what a `use` names,
; `@module.decl` a `mod x;` that pulls in another file, `@function.def`
; a module-level function or an `impl` method, `@type.def` a
; module-level struct, enum, trait, or type alias, and
; `@impl.type`/`@impl.trait` the `impl` block a method sits in.

(use_declaration
  argument: (_) @import.source)

(mod_item
  !body
  name: (identifier) @module.decl)

(source_file
  (function_item
    (visibility_modifier)? @function.visibility
    name: (identifier) @function.name) @function.def)

(source_file
  [
    (struct_item (visibility_modifier)? @type.visibility name: (type_identifier) @type.name)
    (enum_item (visibility_modifier)? @type.visibility name: (type_identifier) @type.name)
    (trait_item (visibility_modifier)? @type.visibility name: (type_identifier) @type.name)
    (type_item (visibility_modifier)? @type.visibility name: (type_identifier) @type.name)
  ] @type.def)

(source_file
  (impl_item
    trait: (_)? @impl.trait
    type: (_) @impl.type
    body: (declaration_list
      (function_item
        (visibility_modifier)? @function.visibility
        name: (identifier) @function.name) @function.def)))
