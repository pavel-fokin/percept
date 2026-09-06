# Map directory

Choose a map by the question you need to answer. `percept maps list`
reports the available maps and their current sizes.

| Map | Use it for | Origin | Entry point |
|---|---|---|---|
| decisions | Lasting commitments, rationale, exceptions, and disagreements. | Cognitive commits in this project's event log. | [Overview](decisions.md), then `maps show decisions --around kind:name`. |
| code | Files, definitions, imports, and affected callers. | The current working tree; rebuilt on each query. | `maps show code --around file:src/main.rs --kind file`. |

A missing claim may still exist in the experience log. A selected
fragment may omit an exception or dependency; expand before concluding
that none exists. Code reflects implementation, not intent.

Read the [shared map skill](../.agents/skills/percept/SKILL.md) before
revising a map or using it to justify a consequential change.
