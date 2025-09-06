# Criticism

Following are the potential missing features & problems to solve for MVP.

1. Log Compaction & State Management
   - Log Compaction: Persistent logs will grow; implement log truncation or snapshotting to compact logs and retain only necessary state.
   - Garbage Collection: Remove old, committed entries after all replicas have learned them.
2. Dynamic Membership
   - Dynamic membership is listed as post-MVP, but real deployments require adding/removing nodes for fault tolerance and scaling.
   - TODO:
3. Network Partition Handling
   - MultiPaxos must handle network partitions gracefully, ensuring safety (no split-brain) and liveness (eventual recovery).
   - TODO: Simulate and test partition scenarios.
4. Leader Election Robustness
   - Ensure leader election handles failures, restarts, and partitions. Consider edge cases like multiple simultaneous leader candidates.
5. Message Ordering & Deduplication
   - Real networks can delay, duplicate, or reorder messages. Transport and protocol logic should handle these cases to avoid inconsistent state.
6. Persistent Storage Consistency
   - Ensure atomicity and consistency of writes, especially during crashes or restarts. Test recovery from partial writes and corruption.
7. Configuration & Deployment
   - MVP should include a way to configure nodes (addresses, roles, transport type) and launch a cluster easily. Provide example configs and scripts.
8. Monitoring & Observability
   - Basic logging, metrics, and error reporting are essential for debugging and validation, even for MVP.
9. Performance & Scalability
   - Benchmark basic throughput and latency; document bottlenecks.
10. Security
    - Note the lack of authentication, encryption, and access control in MVP documentation.
