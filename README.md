# nodus-server-sdk

Server-side protocol primitives for Nodus services. ActivityPub is the core
protocol; every implementation dialect is a separate opt-in crate.

| Crate | Purpose |
| --- | --- |
| `nodus-activitypub-core` | ActivityPub types, actors, notes, collections, activities and WebFinger. No transport, no server, no implementation quirks. |
| `nodus-activitypub-client` | Read-only ActivityPub transport over HTTPS. |
| `nodus-activitypub-sdk` | Facade over the protocol core; `native` adds the reader. |
| `nodus-federation` | HTTP signatures, delivery and inbox policy. |
| `nodus-mastodon-sdk` | Mastodon-compatible endpoint wire types (extension). |
| `nodus-misskey-sdk` | Misskey client API, MiAuth, streaming, reactions and ActivityPub dialect types (extension). |
| `nodus-performance-sdk` | Nodus batch, events and state-sync wire types (extension). |
| `nodus-service-sdk` | Shared-secret authentication for internal gRPC calls. |
| `nodus-server-sdk` | Facade that composes the crates with feature gates. |

The split is intentional:

```
ActivityPub protocol  ->  extension layer  ->  host application
activitypub-core          mastodon              storage, HTTP, policy
activitypub-client        misskey
federation                performance
```

`activitypub-core` must never depend on `mastodon`, `performance`, HTTP or a
database. Extensions must never leak their endpoint shapes into the protocol
types. The facade only re-exports; it does not define behaviour.

```toml
nodus-server-sdk = { path = "../server-sdk/crates/server", features = ["activitypub", "native", "federation", "crypto", "service", "misskey"] }
```

Every internal gRPC call carries `x-nodus-service-secret`; servers validate it in
their interceptor with `nodus_service_sdk::SharedSecret::verify`, clients attach
it with `attach`. Empty or missing secrets are configuration errors, not open
access.

## Misskey extension

`nodus-misskey-sdk` is opt-in and stays outside the protocol core. It models the
Misskey client surface (`/api/*`, MiAuth, `/streaming`) separately from the
Mastodon API, plus the Misskey ActivityPub dialect (`_misskey_content`,
`question`, `renote`, `reactions`).

Rich notes are represented directly: `Note` carries CW, visibility, files,
polls, custom emoji, renotes/replies and reaction counts; `Reaction` validates
Unicode or `:shortcode@host:` values before they reach storage. The SDK does not
render MFM or HTML. A host converts note source into sanitised `content_html`
for federation and lets the shared UI render an allow-listed MFM subset with
CSS animations, honouring `prefers-reduced-motion`.
