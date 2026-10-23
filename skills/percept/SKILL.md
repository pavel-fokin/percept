---
name: percept
description: >-
  Lay out a question or task on the project's maps, and keep them
  current. Use when the user asks to put something on a map, to
  reflect, or to record what was decided or learned.
---

percept keeps maps of what a project understands. A project holds
maps and boards. A board is a space for working on one question or
task: it shows only the maps created on it. A map is created from a
skill in the library, `~/.percept/skills`. The skill says what belongs
on the map and how to write it, and may declare a schema. The map
keeps the schema it was created with; a later edit to the skill
changes its instructions only. `percept describe <map>` prints a map
as markdown:

| Part | Holds |
|---|---|
| Line under the title | what the skill is for |
| Instructions | what belongs on the map and how to write it, as the skill reads now |
| Schema | the map's node kinds with their properties, and edge kinds with the node kinds they link; observed from the map when it has none |

Commands name a map or a board by a bare name in the current project,
or by an absolute path:

| Path | Names |
|---|---|
| `/<project>/<map>` | a map in the project's map list |
| `/<project>/<board>` | a board |
| `/<project>/<board>/<map>` | a map on a board |

Every write prints the full path of what it changed. Reuse that path.

## Choose a map

1. Run `percept show` to see the project's path, maps and boards, and
   `percept show /<project>/<board>` to list a board's maps.
2. To work through a question or task, use the board the user names,
   or create one. Lay the question out across several maps on it, one
   for each way of looking at it:

   ```
   percept new board <board>
   ```

3. When no map fits what the discussion needs, run `percept skills` to
   list the library. Pick a skill by its description and create a map
   from it. One skill can back many maps, so name the map for what it
   will hold:

   ```
   percept new map <map> --skill <skill>
   percept new map /<project>/<board>/<map> --skill <skill>
   ```

   The first puts the map in the project's map list, the second on
   the board only.
4. Run `percept describe <map>`, then `percept show <map>`.

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
5. Use only the kinds and properties the map's schema declares. A schema
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
