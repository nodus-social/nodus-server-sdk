# nodus-server-sdk

Server-side protocol primitives for Nodus services.

| Crate | Purpose |
| --- | --- |
| `nodus-activitypub-sdk` | ActivityPub actors, notes, collections, WebFinger and the public reader. |
| `nodus-federation` | Activities, HTTP signatures, delivery and inbox policy. |
| `nodus-server-sdk` | Facade that re-exports the two with feature gates. |

It deliberately contains no HTTP server, database or client state. Services such
as `nodus-hydrator` compose it with their own storage and transport.

```toml
nodus-server-sdk = { path = "../server-sdk/crates/server", features = ["native", "federation", "crypto"] }
```
