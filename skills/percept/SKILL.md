---
name: percept
description: >-
  Lay out a question or task on the project's maps, and keep them
  current. Use when the user asks to put something on a map, to
  reflect, or to record what was decided or learned.
---

percept keeps maps of what a project understands. A map is created
from a skill in the library, `~/.percept/skills`. The skill says what
belongs on the map and how to write it, and may declare a schema.
`percept describe <map>` prints a map's skill as markdown:

| Part | Holds |
|---|---|
| Line under the title | what the skill is for |
| Instructions | what belongs on the map and how to write it |
| Schema | each node kind with its properties, each edge kind with the node kinds it links; observed from the map when the skill declares none |

## Choose a map

1. Run `percept show` to list the project's maps.
2. When none fits what the discussion needs, run `percept skills` to
   list the library. Pick a skill by its description and create a map
   from it. One skill can back many maps, so name the map for what it
   will hold:

   ```
   percept create <map> <skill>
   ```

3. Run `percept describe <map>`, then `percept show <map>`.

## Write on a map

1. From the discussion, pick what the map's instructions admit. On
   reflect, go through every map of the project.
2. Skip what the map already holds, judged by meaning, not wording.
   Add the rest:

   ```
   percept add <map> <kind> "<title>" --<property> "<value>"
   percept add <map> "<from title>" <edge kind> "<to title>"
   ```

3. Change a node the discussion showed is no longer accurate. Remove a
   node or an edge that no longer holds. Follow the map's
   `instructions` first: a map may forbid both.

   ```
   percept change <map> "<title>" ["<new title>"] --<property> "<value>"
   percept remove <map> "<title>"
   percept remove <map> "<from title>" <edge kind> "<to title>"
   ```

   A change keeps the properties it does not name. Removing a node
   removes its edges.
4. percept refuses an agent's change or removal of what a human wrote.
   Leave it as it is, and tell the user what you would change and why.
5. Use only the kinds and properties the skill declares. A schema
   marked observed is not checked: reuse its kinds and properties, and
   add a new one only when none fits. Leave out a property that has no
   value.
6. Add references with the node when they help a reader check it.
   Repeat `--ref` for each source:

   | Flag | Source |
   |---|---|
   | `--ref 'msg:<text>'` | a project message containing this text, copied verbatim |
   | `--ref 'file:<path>:<from>-<to>'` | a project file's lines, numbered from 1, both bounds included |

   File paths are relative to the project root. Message lookup prefers
   human messages, then agent messages, newest first within each.
   Both forms save the quoted text with the node. On `change`, each
   `--ref` adds to the node's references.
7. Tell the user what you added, changed, and removed, one line per
   node or edge.
