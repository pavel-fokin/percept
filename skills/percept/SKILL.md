---
name: percept
description: >-
  Record what this session settled on the project's maps. Use when the
  user asks to reflect, or to record what was decided or learned.
---

percept keeps maps of what a project understands. A map is declared
by a schema in `.percept/schemas/<map>.toml`:

| Field | Holds |
|---|---|
| `purpose` | what the map is for, in one line |
| `instructions` | what belongs on the map and how to write it |
| `[nodes.<kind>]` | a node kind; its keys are the node's properties |
| `[edges.<kind>]` | an edge kind, with the node kinds it links |

## Reflect

1. Read every schema in `.percept/schemas/`.
2. Run `percept show <map>` for each. If the map is missing, run
   `percept create <map>`.
3. From this session, pick what each map's `instructions` admit.
4. Skip what the map already holds, judged by meaning, not wording.
   Add the rest:

   ```
   percept add <map> <kind> "<title>" --<property> "<value>"
   percept add <map> "<from title>" <edge kind> "<to title>"
   ```

5. Change a node the session showed is no longer accurate. Remove a
   node or an edge that no longer holds. Follow the map's
   `instructions` first: a map may forbid both.

   ```
   percept change <map> "<title>" ["<new title>"] --<property> "<value>"
   percept remove <map> "<title>"
   percept remove <map> "<from title>" <edge kind> "<to title>"
   ```

   A change keeps the properties it does not name. `--<property> ""`
   drops one. Removing a node removes its edges.
6. percept refuses an agent's change or removal of what a human wrote.
   Leave it as it is, and tell the user what you would change and why.
7. Use only the kinds and properties the schema declares. Leave out a
   property that has no value.
8. Add references with the node when they help a reader check it.
   Repeat `--ref` for each source:

   | Flag | Source |
   |---|---|
   | `--ref 'msg:<text>'` | a project message containing this text, copied verbatim |
   | `--ref 'file:<path>:<from>-<to>'` | a project file's lines, numbered from 1, both bounds included |

   File paths are relative to the project root. Message lookup prefers
   human messages, then agent messages, newest first within each.
   Both forms save the quoted text with the node. On `change`, each
   `--ref` adds to the node's references.
9. Tell the user what you added, changed, and removed, one line per
   node or edge.
