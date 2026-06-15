-- Full context snapshot after each node finishes, so a single node (and its
-- descendants) can be retried/resumed without re-running upstream.
ALTER TABLE run_steps ADD COLUMN ctx_state TEXT;
