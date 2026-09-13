# Cognitive record

This project uses percept as its cognitive record. The event log is
immutable experience. The decisions and concepts maps are compact claims
you build from that experience.

At the start of each task, run `percept start`, then inspect relevant map
fragments and search the log when the map points to missing context. Treat
earlier requirements as history when a newer prompt changes them.

Record each settled design decision in the decisions map before finishing.
Cite the prompt or other event that supports the claim. When a decision
replaces an earlier one, add the new claim beside it and connect it with a
`supersedes` edge. Keep concepts and their relationships only when they
make the implementation easier to recover in a fresh session.
