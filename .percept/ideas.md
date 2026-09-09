# ideas

Folded from the percept log for this project and rerendered on every write. Change it with `percept maps`, not by hand.

## idea
- "a percept maps record verb, taking a document on stdin to add several nodes and edges in one call" (model): why: "today recording a decision with its options takes one add-node/add-edge call per node and per edge - six calls for the short-id decision this session, one of which needed shell-escaping an apostrophe in a --prop value. Once short ids (this session's other change) exist, referencing an EXISTING node gets cheap, but a NEW node still needs one add-node call before it has an id to link, so the round-trip count stays. Deferred pending evidence that short ids alone don't remove enough of the pain from real use - see docs/clients.md change 3."
  sources: 01a085d9-3ef8-7f81-af6a-2262e74ee00c
