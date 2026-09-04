; Capture names shared by every language's query, so one reader in
; `code` drives them all: `@import.source` is what a `use` names,
; `@module.decl` a `mod x;` that pulls in another file, `@symbol.def`
; a module-level function, struct, enum, trait, or type alias, or an
; `impl` method - its node kind says which - with `@symbol.name` and
; `@symbol.visibility`, and `@impl.type`/`@impl.trait` the `impl`
; block a method sits in.
;
; Every pattern is anchored under `source_file`: an item inside an
; inline `mod x { }` belongs to that module, not the file, and a `use`
; there would resolve against the wrong base.

(source_file
  (use_declaration
    argument: (_) @import.source))

(source_file
  (mod_item
    !body
    name: (identifier) @module.decl))

(source_file
  [
    (function_item (visibility_modifier)? @symbol.visibility name: (identifier) @symbol.name)
    (struct_item (visibility_modifier)? @symbol.visibility name: (type_identifier) @symbol.name)
    (enum_item (visibility_modifier)? @symbol.visibility name: (type_identifier) @symbol.name)
    (trait_item (visibility_modifier)? @symbol.visibility name: (type_identifier) @symbol.name)
    (type_item (visibility_modifier)? @symbol.visibility name: (type_identifier) @symbol.name)
  ] @symbol.def)

(source_file
  (impl_item
    trait: (_)? @impl.trait
    type: (_) @impl.type
    body: (declaration_list
      (function_item
        (visibility_modifier)? @symbol.visibility
        name: (identifier) @symbol.name) @symbol.def)))
