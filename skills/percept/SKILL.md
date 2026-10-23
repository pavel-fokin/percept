---
name: percept
description: >-
  Record what this session settled on the project's maps. Use when the
  user asks to reflect, or to record what was decided or learned.
---

percept keeps maps of what a project understands. Each map has a
definition. `percept describe <map>` prints it as markdown:

| Part | Holds |
|---|---|
| Line under the title | what the map is for |
| Instructions | what belongs on the map and how to write it |
| Schema | each node kind with its properties, each edge kind with the node kinds it links; observed from the map when the definition declares none |

## Reflect

1. Run `percept show` to list the project's maps.
2. For each map, run `percept describe <map>`, then `percept show <map>`.
   If a map you need is missing, tell the user. They choose its
   definition file and create the map.
3. From this session, pick what each map's instructions admit.
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

   A change keeps the properties it does not name. Removing a node
   removes its edges.
6. percept refuses an agent's change or removal of what a human wrote.
   Leave it as it is, and tell the user what you would change and why.
7. Use only the kinds and properties the definition declares. A schema
   marked observed is not checked: reuse its kinds and properties, and
   add a new one only when none fits. Leave out a property that has no
   value.
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
