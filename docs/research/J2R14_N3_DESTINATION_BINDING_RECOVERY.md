# J2R14 N3 Destination Binding and Recovery

J2R14 repairs the failed-run boundary without generating media. The consumed
J2R13 run is terminal, its retired queue is not reusable, and the recovery queue
uses a fresh run namespace.

The destination contract is explicit:

1. validate an approved empty parent directory;
2. navigate the Save panel to that parent;
3. verify the panel parent identity;
4. enter and verify a leaf filename (`carrier.mp4`);
5. verify the resolved parent-plus-leaf destination;
6. stop at `READY_TO_CONFIRM` and disable Save in this milestone.

The contract rejects absolute paths in the leaf field, path separators,
traversal, wrong folders, and parent substitution. Four fresh Logic processes
reached the verified state for S_FL, S_FR, D_SWAP, and a repeated S_FL run; all
four rehearsals cancelled before Save and produced no media.

This is a producer-control and recovery milestone only. It does not admit N3
evidence, alter JOC semantics, or change `SemanticBindingState::Unresolved`.
