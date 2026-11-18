# Federation parity: our ActivityPub surface vs. Mastodon

Audit of what Nodus serves and sends over ActivityPub, compared with what Mastodon (4.x) sends and
what it requires to interoperate. Every claim carries a `file:line`.

Path prefixes used below:

| key | path |
| --- | --- |
| `api/` | `nodus-backend/crates/api/src/` |
| `store/` | `nodus-backend/crates/store/src/` |
| `apcore/` | `server-sdk/crates/activitypub-core/src/` |
| `client/` | `server-sdk/crates/activitypub-client/src/client.rs` |
| `be-fed/` | `nodus-backend/crates/federation/src/lib.rs` (the verifier the server actually uses) |
| `sdk-fed/` | `server-sdk/crates/federation/src/` (**dead code**, not linked by `nodus-backend`) |

`sdk-fed` is the crate with the nicer `Delivery`/`SignedRequest` abstractions (`sdk-fed/delivery.rs:326-343`,
`sdk-fed/signature.rs:135-151`) and it is never referenced: `nodus-backend/Cargo.toml:19` maps
`nodus-server-federation` to `nodus-backend/crates/federation`, and only `be-fed` is called
(`api/outbox.rs:307`, `api/activitypub.rs:290-361`). Anything "we" do is in `api/` + `be-fed/`.

Server-side AP lives in one file, `api/activitypub.rs` (1084 lines), plus `api/outbox.rs` (delivery
queue producer) and `api/remote.rs` (signed fetcher). Protocol types are `apcore`; the read-only
fetcher is `client/`, whose module doc is literally `//! Public reads over HTTPS. No signing, no
delivery.` (`client/client.rs:1`).

---

## 1. Discovery

| Requirement | Mastodon | Nodus | Status | Evidence |
| --- | --- | --- | --- | --- |
| `/.well-known/webfinger?resource=acct:user@host` | 200, JRD, `rel=self` with `application/activity+json` | 200, `application/jrd+json` | ok | `api/activitypub.rs:91,108-126,899-906`; `apcore/webfinger.rs:77-86` |
| `resource=https://host/users/x` | supported | supported | ok | `api/activitypub.rs:884-897` |
| unknown account | 404 | 404 `No such account` | ok | `api/activitypub.rs:112-116` |
| `/.well-known/host-meta` | served | **absent** | cosmetic | repo grep: only `nodus-backend/crates/auth/src/lib.rs:147` (`oauth-authorization-server`) |
| `/.well-known/nodeinfo` (+ `2.0`/`2.1`) | served | **absent** | visible | no route; `api/lib.rs:124-143` and `api/activitypub.rs:89-101` have none |
| `Vary: Accept`, `ld+json` negotiation | varies | always `application/activity+json`, no `Vary` | cosmetic | `api/activitypub.rs:899-906` |
| webfinger links: `profile` page | yes | yes (`text/html`) | ok | `apcore/webfinger.rs:77-86` |
| `aliases` = actor id | yes | yes | ok | `apcore/webfinger.rs:77-86` |
| `rel` query honoured | yes | ignored, always both links | cosmetic | `api/activitypub.rs:108-126` |

Discovery is the one area with no functional blocker: Mastodon resolves actors purely through
WebFinger (`client/client.rs:71`) and never needs host-meta. NodeInfo absence is what breaks
fediverse directory listings and `tootctl`-style tooling, not federation.

## 2. Actor document

Emitted by `api/activitypub.rs:834-868` through `apcore/actor.rs:173-234`.

| Field | Mastodon | Nodus | Status | Evidence |
| --- | --- | --- | --- | --- |
| `@context` | AS + `security/v1` + toot extensions | same, plus `featured`, `featuredTags`, `alsoKnownAs`, `movedTo`, `Hashtag`, `Emoji`, `blurhash`, `focalPoint` declared but unused | ok | `apcore/context.rs:20-44` |
| `id`, `type: Person` | yes | yes | ok | `api/activitypub.rs:837-838` |
| `preferredUsername`, `name`, `summary` | yes | yes (summary is raw account HTML) | ok | `api/activitypub.rs:839-841` |
| `url` | profile URL | `/@username` | cosmetic | `api/activitypub.rs:842`; that path is not an API route (`api/activitypub.rs:89-101`) so it only works if the web front end serves it |
| `icon` / `image` | `Image` objects | `Image` with `url` only | ok | `apcore/actor.rs:173-234` |
| `followers` / `following` / `outbox` / `inbox` | yes | yes | ok | `api/activitypub.rs:851-854` |
| `endpoints.sharedInbox` | yes | yes, always instance-wide `/inbox` | ok | `api/activitypub.rs:855`; handler `api/activitypub.rs:259-266` |
| `publicKey.id` = `{actor}#main-key`, `owner` = actor id, PEM | yes | yes; key minted on first fetch | ok | `api/activitypub.rs:856-857,775-799` |
| `published` | yes | yes | ok | `api/activitypub.rs:858-862` |
| `discoverable`, `indexable`, `manuallyApprovesFollowers` | yes | yes, from account flags | ok | `api/activitypub.rs:863-865` |
| `featured` (pinned posts), `featuredTags` | yes | **missing** | visible | `extra: Map::default()` at `api/activitypub.rs:866`; no pinned-post concept in `api/highlights.rs` |
| `attachment` (`PropertyValue` profile fields) | yes | **missing**; `field` rows exist for the client API only | visible | `api/dto.rs:69,121`; never in `actor_document` |
| `alsoKnownAs`, `movedTo` | yes | missing (no migration feature) | visible | `api/activitypub.rs:866` |
| `streams` | (some servers) | missing; only a Misskey-shaped websocket exists | cosmetic | `api/misskey.rs:53,812-840`; `api/dto.rs:366` |
| `endpoints.oauthAuthorizationEndpoint` / `oauthTokenEndpoint` | yes | missing | cosmetic | `apcore/actor.rs:173-234` emits `endpoints.sharedInbox` only |
| actor fetch requires no signature | yes | yes (anyone may fetch) | ok | `api/activitypub.rs:128-134` |
| Accept-header negotiation | `activity+json` or `ld+json` | ignores Accept | cosmetic | `api/activitypub.rs:899-906` |

## 3. Inbox

Entry points: `POST /users/{username}/inbox` and `POST /inbox` (`api/activitypub.rs:92,97`), both
through `accept_activity` (`api/activitypub.rs:268-288`), then `ingest_activity`
(`api/activitypub.rs:363-405`). Response is always `202 Accepted` with an empty body; the body limit
is 1 MiB (`api/activitypub.rs:99`).

| Activity | Mastodon sends it for | Nodus handling | Status | Evidence |
| --- | --- | --- | --- | --- |
| `Follow` | following us | `social::follow`; target locked ⇒ `pending`, else `accepted`; **no `Accept` emitted** | blocker | `api/activitypub.rs:611-620`; `store/social.rs:132-171`; no `deliver_follow_response` call outside `api/follow_requests.rs:50,66` |
| `Accept` / `Reject` | answering our `Follow` | ignored (`_ => {}`) | blocker | `api/activitypub.rs:363-405` |
| `Undo{Follow}` | unfollow | `unfollow_local`, requires same actor else 403 | ok | `api/activitypub.rs:372-378` |
| `Undo{Like}` / `Undo{Announce}` | un-favourite / un-boost | falls into the generic `object id ⇒ unfollow_local` branch | visible | `api/activitypub.rs:380-390` |
| `Delete{Note}` | deleting a post | `delete_remote` after `same_origin(id, actor)` | ok | `api/activitypub.rs:392-400` |
| `Delete{Person}` | account deletion | ignored | visible | `api/activitypub.rs:363-405` |
| `Create{Note}` | new post | `ingest_note` (Value objects only) | partial | `api/activitypub.rs:402-404,629-687` |
| `Create{Question}` (poll) | poll | no poll concept anywhere | blocker | `api/statuses.rs` has none; `api/dto.rs:277` hardcodes `poll: None` |
| `Update{Note}` | edits | ignored | visible | `api/activitypub.rs:363-405` |
| `Like` | favourite of our post | ignored | blocker | `api/activitypub.rs:363-405` |
| `Announce` | boost of our post | ignored (also when object is a URL) | blocker | `api/activitypub.rs:363-405` |
| `Move` | account migration | ignored | visible | `api/activitypub.rs:363-405` |
| `Block` | remote blocked us | ignored; no follow cleanup | visible | `api/activitypub.rs:363-405` |
| `Flag` | report forwarded to us | ignored | visible | `api/activitypub.rs:363-405`; `api/admin/reports.rs` never builds a `Flag` |
| `Add`/`Remove` (pins) | pinned/unpinned | ignored | cosmetic | `api/activitypub.rs:363-405` |

### Signature verification, digest, idempotency

`api/activitypub.rs:290-361` delegates to `prepare` + `verify_prepared` in `be-fed/`.

| Property | Mastodon | Nodus | Status | Evidence |
| --- | --- | --- | --- | --- |
| Scheme | draft-cavage `Signature:`; 4.4+ also RFC 9421 `Signature-Input` | draft-cavage only; `Signature-Input` is not parsed anywhere | visible (blocks 4.4 ed25519 peers) | `be-fed/lib.rs:26,61-110,178-222` |
| Algorithms | `rsa-sha256` (+ `hs2019`/ed25519 in 4.4) | `rsa-sha256` only, else `UnsupportedAlgorithm` | visible | `be-fed/lib.rs:178-222` |
| Required signed headers | `(request-target) host date digest` | exactly these four must be present | ok | `be-fed/lib.rs:26,178-222` |
| Extra signed headers (e.g. `content-type`) | allowed | allowed (only presence of the four is checked) | ok | `be-fed/lib.rs:61-110` |
| Clock skew | ±12 h | **±12 min** | visible | `be-fed/lib.rs:23` |
| Digest | `SHA-256=…` over body | SHA-256 only, constant-time compare, header must be signed | ok | `be-fed/lib.rs:135-150,178-222` |
| Key lookup | actor `publicKey` via `keyId` | `keyId` base must equal `activity.actor`, actor doc must agree, PEM cached on the account | ok | `api/activitypub.rs:290-361` |
| Actor/`keyId` host binding | key host must match actor host | not checked (only `same_origin` on the note/`Delete` object) | cosmetic | `api/activitypub.rs:290-361` |
| Signed by a different actor than `actor` | tolerated for relayed activities | rejected 401 `signing key does not match activity actor` | visible | `api/activitypub.rs:290-361` |
| Blocked domain | — | 403 `domain is blocked` (severity ignored, see §8) | visible | `api/activitypub.rs:422-430`; `store/moderation.rs:341-347` |
| Malformed JSON | — | 422 | ok | `api/activitypub.rs:290-361` |
| Idempotency | dedupe by activity id | no activity ledger; relies on `ON CONFLICT (uri)` for notes and follow upserts | visible | `store/statuses.rs:114-146`; `store/social.rs:132-171` |
| Retry of inbound | sender retries on non-2xx | we retry nothing; `202` is returned even for ignored kinds | ok | `api/activitypub.rs:268-288` |
| Response body | empty | empty | ok | `api/activitypub.rs:268-288` |

Inbound `Note` ingestion keeps `tag` (mentions, hashtags, custom emoji) and `attachment`
(media URLs), stores the visibility its addressing states, and serves tags/mentions/emojis
through the status DTO (`api/activitypub.rs`, `store_note_extras`; `api/statuses.rs`,
`decorate`). A peer `Question` is stored as a poll (`api/activitypub.rs`,
`store_note_poll`).

## 4. Collections

| Routine | Shape | Status | Problem | Evidence |
| --- | --- | --- | --- | --- |
| outbox (no `?page`) | `OrderedCollection` + `totalItems` + `first=?page=true` | visible | `totalItems` = `statuses_count` **including private/direct**, while the page query filters to `public,unlisted` | `api/activitypub.rs:142-194` vs `store/statuses.rs:226-249` |
| outbox page | `OrderedCollectionPage` + `partOf` + `totalItems` + `orderedItems` + `next` (`?page=true&max_id=`) | cosmetic | no `last`, no `prev`; page size fixed at 20, `?limit` ignored | `api/activitypub.rs:142-194` |
| outbox items | `Create{Note}` only | visible | boosts are stored as rows with empty `content` (`store/statuses.rs:709-717`) and rendered as an **empty Note**, never as `Announce` | `api/activitypub.rs:703-710,750-769` |
| followers / following | `OrderedCollection` + `totalItems` + `first=?page=true` | blocker | **no page branch**: `?page=true` returns the same header, `orderedItems` never served | `api/activitypub.rs:196-232` |
| `orderedItems` vs `items` | Mastodon expects `orderedItems` | we emit `orderedItems`, and accept either on read | ok | `apcore/collection.rs:75-110` |
| inbox collection (`GET`) | Mastodon serves it | route is POST-only ⇒ 405 | cosmetic | `api/activitypub.rs:97` |
| page ids | stable, absolute | `first` = `?page=true`, page id = `?page=true&max_id=…` | cosmetic | `api/activitypub.rs:142-194` |

## 5. Outbound delivery

Queue: `federation_delivery` via `store/delivery.rs`, worker `api/outbox.rs:360-386` (20 per tick,
20 s tick).

| Property | Mastodon | Nodus | Status | Evidence |
| --- | --- | --- | --- | --- |
| shared inbox | preferred | `COALESCE(shared_inbox_url, inbox_url)` | ok | `store/delivery.rs:136-164` |
| recipients | followers + mentions + `cc` | followers only | blocker | `api/outbox.rs:94-140` |
| visibility gate | none (DMs/replies federate) | returns unless `public`/`unlisted`; comment claims a mention fan-out that does not exist | blocker | `api/outbox.rs:101-110` (comment at `106-107`) |
| retries | exponential, many attempts | 8 attempts, 30 s·2ⁿ capped 1 h, then silently closed | visible | `store/delivery.rs:15,31-34,109-132` |
| dedupe | per inbox+activity | `ON CONFLICT (inbox_url, activity_id) DO NOTHING` | ok | `store/delivery.rs:40-70` |
| required headers | `Host`, `Date`, `Digest`, `Signature` | all four signed over `(request-target) host date digest`; `Content-Type`/`Accept` = `application/activity+json` | ok | `api/outbox.rs:296-357`; `be-fed/lib.rs:317-346` |
| `Host` derivation | URL host | URL host (no manual override) | ok | `api/outbox.rs:296-357` |
| UA | `http.rb` style | `nodus/{version}` | cosmetic | `api/outbox.rs:296-357` |
| activity ids | stable, reused on `Undo` | freshly minted `{actor}#{label}/{ulid}` **on every call**, including the inner activity of every `Undo` | blocker | `api/outbox.rs:41-66` |
| `Accept`/`Reject` object | the original `Follow` activity | the requester's **actor URL** (`object` is a string) | blocker | `api/outbox.rs:203-229` |
| `Like`/`Announce` object | the **status** URI | the author's **actor** URI | blocker | `api/outbox.rs:249-262` |
| `Delete{Note}` body | `Tombstone` + `published`/`atomUri` | `{id, type: Tombstone}` only | cosmetic | `api/outbox.rs:158-165` |
| `Update{Person}` on profile change | yes | **never sent** | blocker | `api/accounts.rs:46-87` emits nothing |
| `Delete{Person}` on account deletion | yes | **never sent** | visible | no caller in `api/` |
| `Announce` of own post (self-boost) | yes | never; reblog only sends `Announce` to the remote author | visible | `api/statuses.rs:262-292` |
| `Move`, `Block`, `Flag`, `Add`/`Remove` | yes | never sent | visible | `api/accounts.rs:292-303`; `api/admin/reports.rs` |
| ordering | FIFO per inbox | FIFO by `created_at`, no per-inbox serialisation; a `Create` may still go out after its `Delete` | visible | `store/delivery.rs:73-94` |
| failure visibility | admin UI | `pending` listing only | cosmetic | `store/delivery.rs:210-231` |

Note the asymmetry in `deliver_reaction`: it dereferences the *author* and posts to `inbox_of(target_id)`
— i.e. likes and boosts of a remote post reach the author but name the author as the object, so the
remote side has nothing to attach them to (`api/outbox.rs:239-268`).

## 6. Objects

`Note` is built by `note_document` (`api/activitypub.rs:712-770`) and serialised by
`apcore/note.rs:158-231`.

| Field | Mastodon | Nodus | Status | Evidence |
| --- | --- | --- | --- | --- |
| `id`, `url` | status permalink | `/statuses/{id}`, `/@{user}/{id}` | ok | `api/activitypub.rs:751-754` |
| `attributedTo` | actor id | yes | ok | `api/activitypub.rs:764` |
| `to`/`cc` with `as:Public` | public: `to=[Public] cc=[followers]`; unlisted reversed | same | ok | `api/activitypub.rs:721-731` |
| non-public notes | served to followers | 404 for everyone (no signature check on `GET /statuses/{id}`) | visible | `api/activitypub.rs:234-247` |
| `content` | sanitised HTML | sanitised HTML, but local `render` emits no `<a>` for mentions/hashtags | visible | `api/statuses.rs:642-659`; `api/activitypub.rs:755` |
| `tag` (`Mention`, `Hashtag`, `Emoji`) | populated | always `[]` | blocker | `api/activitypub.rs:767`; `apcore/note.rs:158-231` |
| `inReplyTo` | parent URI | local parent URL only; no parent fetch | visible | `api/activitypub.rs:715-720,761` |
| `replies` | collection URL that resolves | `{id}/replies`, **no such route** | visible | `api/activitypub.rs:762`; router `api/activitypub.rs:89-101` |
| `sensitive`, `summary` | summary only for CW | matches (empty summary omitted) | ok | `api/activitypub.rs:757-758`; `apcore/note.rs:158-231` |
| `attachment[].type` | `Image`/`Video`/`Audio`/`Document` | `Image`/`Video`/`Document` (audio becomes `Document`) | cosmetic | `api/activitypub.rs:733-749` |
| attachment `url` + `icon` | distinct (preview) | identical `/media/{id}` | cosmetic | `api/activitypub.rs:743-744` |
| `blurhash`, `focalPoint` | yes | missing, though the media row carries `blurhash` | visible | `store/media.rs:21`; `api/activitypub.rs:733-749` |
| `contentMap`/`language`, `conversation`, `atomUri` | yes | missing | cosmetic | `apcore/note.rs:56-89` |
| `Question`/polls | yes | inbound stored as a read-only poll; outbound unsupported (compose has no polls) | visible | `api/activitypub.rs`, `store_note_poll` |
| `Article`/`Video` objects | rendered | `Note::from_json` accepts any `type`, fields dropped; `Announce` (object as URL) never fetched | visible | `apcore/note.rs:114-151`; `api/activitypub.rs:402-404` |

## 7. Fetch-on-demand

Only `GET /api/v1/hydrate/account` and `GET /api/v1/hydrate/status` exist (`api/hydrate.rs:165-169`).

| Capability | Nodus | Status | Evidence |
| --- | --- | --- | --- |
| resolve handle ⇒ actor + first outbox page | yes | ok | `api/hydrate.rs:37-75,119-159` |
| fetch one status by URL | yes, single note, `same_origin` checked | ok | `api/hydrate.rs:77-117` |
| thread / ancestors | **no** (`client/replies` at `client/client.rs:179` is unused) | visible | `api/hydrate.rs` |
| parent lookup for a reply | only an already-stored row by `in_reply_to` URI | visible | `api/activitypub.rs:659-662` |
| media | URLs only, never fetched or proxied; remote attachments are dropped at ingestion | visible | `api/activitypub.rs:629-687` |
| signed GETs | none — `client/` sends no `Signature` | blocker (AUTHORIZED_FETCH peers answer 401) | `client/client.rs:1,52` |
| pinned DNS / SSRF guard | yes | ok | `client/client.rs:307,362`; `api/remote.rs:189-218` |

## 8. Blocks, mutes, domain blocks

| Control | Mastodon | Nodus | Status | Evidence |
| --- | --- | --- | --- | --- |
| local user blocks remote | sends `Block`, severs follow | local row only; **no activity**, no follow cleanup | blocker | `api/accounts.rs:292-303` |
| inbound `Block` | honoured | ignored | visible | `api/activitypub.rs:363-405` |
| domain block `suspend` | drop traffic | 403 on inbox, actor fetch, note ingest | ok | `api/activitypub.rs:422-430,509-521` |
| domain block `silence` | accept, hide from public timelines | identical to `suspend`: `blocked_domain` ignores `severity` | visible | `store/moderation.rs:341-347`; no `severity` read anywhere outside `api/admin/federation.rs` |
| `reject_media` | drop remote media | stored, never enforced (we never fetch remote media anyway) | visible | `store/moderation.rs:233-246`; `api/admin/federation.rs:80-93` |
| `reject_reports` | drop reports about that domain | stored, never enforced | visible | `api/admin/reports.rs` has no domain check |
| domain blocks propagated to peers | Mastodon does not propagate either | n/a | ok | — |
| mutes | local only | local only, plus streaming filter | ok | `api/accounts.rs:318-346` |

## 9. What Mastodon will reject or ignore

| We send | Mastodon reaction | Evidence |
| --- | --- | --- |
| `Accept`/`Reject` whose `object` is a URL string | dropped: the handler requires the `Follow` activity (a Hash) and matches its `object`/id; the follow request never resolves | `api/outbox.rs:203-229` |
| `Like`/`Announce` with `object` = actor URI | ignored: no status to attach; the peer's favourite/boost never appears | `api/outbox.rs:249-262` |
| `Undo` with a **new** inner activity id | ignored: `Undo` is matched to the earlier activity by id, which never matches a fresh ULID | `api/outbox.rs:41-66,178-200,232-269` |
| `Create{Note}` with empty `content` (boosts in the outbox) | empty, unrenderable status | `store/statuses.rs:709-717`; `api/activitypub.rs:703-710` |
| `Note.replies` pointing at a 404 | thread fetch fails silently | `api/activitypub.rs:762` |
| followers collection that never pages | "load more" loops, follower list stays empty | `api/activitypub.rs:196-232` |
| signature with only 4 headers / `rsa-sha256` required | fine for Mastodon ≤4.3, 401 from 4.4 ed25519-only peers | `be-fed/lib.rs:178-222` |
| activity signed by an actor other than `actor` | rejected by us, so relayed/forwarded activities are dropped | `api/activitypub.rs:290-361` |

---

## Mismatches (severity ranked)

| # | Severity | Mismatch | Evidence | Test that would prove it |
| --- | --- | --- | --- | --- |
| 1 | blocker | Inbound `Follow` is accepted locally but no `Accept` is ever delivered, so a Mastodon follow stays "pending" forever | `api/activitypub.rs:611-620`; `api/outbox.rs:178-200` | Mastodon follows us: `POST /users/alice/inbox {Follow}` → expect 202 **and** an `Accept` POST to the follower inbox whose `object.id` == the `Follow` id |
| 2 | blocker | `Accept`/`Reject` carry the requester's actor URL, not the `Follow` activity, so approved follow requests never resolve on Mastodon | `api/outbox.rs:203-229` | Approve a pending follow: `POST /api/v1/follow_requests/{id}/authorize` → the peer's inbox receives `Accept{object.type == "Follow"}` |
| 3 | blocker | `Like`/`Announce` point at the author actor instead of the note URI | `api/outbox.rs:249-262` | Favourite a remote post: `POST /api/v1/statuses/{id}/favourite` → outbound `Like.object` must equal the remote note's `id` |
| 4 | blocker | Every `Undo` (and its inner activity) gets a fresh id, so unfollow/unfavourite/unboost are ignored by the peer | `api/outbox.rs:41-66` | Unfollow a Mastodon account: `POST /api/v1/accounts/{id}/unfollow` → `Undo{object.id}` must equal the `Follow.id` sent earlier (same `federation_delivery` row id) |
| 5 | blocker | Replies, DMs and mention fan-out never federate (visibility gate + followers-only recipients) | `api/outbox.rs:101-110,143-150` | Reply to a remote user: `POST /api/v1/statuses` → expect a `Create` in the mentioned actor's inbox (not just followers) |
| 6 | blocker | Inbound `Like`, `Announce`, `Update`, `Move`, `Block`, `Flag` are silently dropped, so remote favourites/boosts of our posts never register | `api/activitypub.rs:363-405` | Peer favourites our post: `POST /inbox {Like, object: our note}` → `favourites_count` becomes 1 and a `favourite` notification appears; same for `Announce` → `reblogs_count` |
| 7 | visible | Outbound `Create` still drops audience nuance; inbound now keeps `tag`/`attachment` and the stated visibility | `store/statuses.rs:114-146`; `api/activitypub.rs`, `store_note_extras` | Hydrate a followers-only remote post: `GET /api/v1/hydrate/status?url=…` → stored visibility must stay `private`, its images/mentions must survive |
| 8 | blocker | Outbound `Create` advertises no `tag` at all, so mentioned users are never notified by the peer and hashtags never link | `api/activitypub.rs:767` | Mention `@bob@remote`: our `Create` must contain `tag:[{type:Mention,href:…}]`; the remote must show/notify the mention |
| 9 | blocker | All outbound reads are unsigned, so AUTHORIZED_FETCH instances refuse our actor/outbox fetches | `client/client.rs:1,52` | Fetch a Mastodon 4.3+ actor with `AUTHORIZED_FETCH=on`: our `ActivityPub::actor_url` returns 401 and hydration fails |
| 10 | blocker | Our `GET`s are unsigned too, so we cannot resolve or hydrate from authorized-fetch peers; no thread fetch at all | `api/hydrate.rs:77-117`; `api/hydrate.rs` (no replies/ancestors) | `GET /api/v1/hydrate/account?acct=user@authfetch.test` → 200 with the outbox page, and a reply to a remote post shows its parent chain |
| 11 | blocker | Followers/following collections never serve items | `api/activitypub.rs:196-232` | `GET /users/alice/followers?page=true` → `orderedItems` must be non-empty when the account has followers |
| 12 | visible | No `featured`, `featuredTags`, `attachment` (profile fields), `alsoKnownAs`, `movedTo` in the actor doc | `api/activitypub.rs:834-868` | `GET /users/alice` → `featured` resolves and pinned posts appear on the peer's profile; profile fields appear as `PropertyValue` |
| 13 | visible | Outbox `totalItems` counts private/direct statuses while pages omit them | `api/activitypub.rs:156,190` vs `store/statuses.rs:226-249` | Create 1 public + 3 direct posts, `GET /users/alice/outbox` → `totalItems` must equal `orderedItems` total (1) |
| 14 | visible | Boosts appear in the outbox as empty `Note`s instead of `Announce` | `store/statuses.rs:709-717`; `api/activitypub.rs:703-710` | Boost a post, `GET /users/alice/outbox?page=true` → item must be `Announce{object: <original id>}` |
| 15 | visible | `Note.replies` is a 404 route | `api/activitypub.rs:762,89-101` | `GET /statuses/{id}/replies` → 200 `OrderedCollectionPage` of replies |
| 16 | visible | No `Update{Person}` on profile edit, no `Delete{Person}` on account deletion | `api/accounts.rs:46-87` | `PATCH /api/v1/accounts/update_credentials` → peer receives `Update{Person}`; `DELETE /api/v1/accounts` → `Delete{Person}` |
| 17 | visible | User blocks and reports are not federated (`Block`, `Flag` never sent; inbound `Block` ignored) | `api/accounts.rs:292-303`; `api/admin/reports.rs` | Block a remote account: the peer receives `Block` and drops the follow; peer sends `Block` → our follow row disappears |
| 18 | visible | Domain-block `silence` behaves exactly like `suspend`; `reject_media`/`reject_reports` are inert | `store/moderation.rs:341-347`; `api/activitypub.rs:422-430` | Block a domain with `severity=silence, reject_reports=true`, then `POST /inbox {Create}` → expect 202 (accepted, hidden), and a report about that domain → 422 |
| 19 | visible | Signature policy is stricter than the ecosystem: `rsa-sha256` only, ±12 min, digest mandatory, no RFC 9421/ed25519 | `be-fed/lib.rs:23,26,178-222` | Mastodon 4.4 with an ed25519 key posts a `Follow` → expect 202; today it is 401 `unsupported algorithm` |
| 20 | visible | Inbound `Create{Question}` becomes a poll, but `Article` still degrades to a bare text `Note`; outbound `Question` is unsupported | `api/activitypub.rs` | `POST /inbox {Create{Question}}` → status with `poll` carrying the peer's options/counts |
| 21 | cosmetic | No NodeInfo, no host-meta, no `Vary: Accept`, no `Content-Type` negotiation, no ETag on actors | §1 table | `GET /.well-known/nodeinfo` → 200 `{"links":[{"rel":"…/2.0","href":…}]}` |
| 22 | cosmetic | `Note.url` (`/@{user}/{id}`) and `Actor.url` (`/@{user}`) are not API routes | `api/activitypub.rs:754,842` | `GET /@alice` → 200 HTML profile (must be served by the web front end) |
| 23 | cosmetic | No `blurhash`/`focalPoint` on attachments; audio typed `Document`; preview == full-size URL | `api/activitypub.rs:733-749` | `GET /statuses/{id}` for a post with an image + blurhash → attachment carries `blurhash` |
| 24 | cosmetic | `Delete{Note}` lacks `published`/`atomUri`; outbox page has no `last`/`prev` | `api/outbox.rs:158-165`; `api/activitypub.rs:142-194` | `GET /users/alice/outbox?page=true` → page has `last`; a `Delete` carries `published` |
| 25 | cosmetic | Dead `server-sdk/crates/federation` crate duplicates the live verifier and will drift | `sdk-fed/signature.rs:135-151`; `sdk-fed/delivery.rs:326-343`; `nodus-backend/Cargo.toml:19` | `cargo tree -i nodus-federation` → nothing depends on it; delete or wire it |

### Blockers in one line each

1. No `Accept` for an inbound `Follow` → every Mastodon follow stays "pending".
2. `Accept`/`Reject` name the actor, not the `Follow` activity → approvals never land.
3. `Like`/`Announce` name the author, not the note → remote favourites/boosts are unattachable.
4. `Undo` re-mints ids → unfollow/unfav/unboost silently ignored.
5. Only public/unlisted `Create`s go to followers → replies and DMs never federate.
6. `Like`/`Announce`/`Update`/`Move`/`Block`/`Flag` inbound are dropped wholesale.
7. Outbound notes carry no `tag`, so remote mentions never notify (inbound tags do survive).
8. Outbound notes carry no `tag`, so remote mentions never notify.
9. Our fetches are unsigned → authorized-fetch peers 401 us.
10. No thread/ancestor fetch; `replies` URL is a 404.
11. Followers/following collections never serve items.

## Status after the first pass

Signed but unverified: delivery signing already existed before this pass
(`nodus-backend/crates/api/src/outbox.rs`, per-account key), and outbound reads are now
signed as well — `nodus-backend/crates/api/src/state.rs` loads one instance key from
`NODUS_FEDERATION_KEY_PATH` (or `NODUS_FEDERATION_KEY`) and
`crates/api/src/activitypub.rs` serves the matching `{instance}/actor` with
`publicKey.id = {instance}/actor#main-key`. That removes the authorized-fetch 401 in
principle; no live peer has confirmed it accepts our signature yet, and an instance with
no key still reads unsigned.

Fixed and deployed:

- an inbound `Follow` is answered with an `Accept` naming the follow it answers
  (`crates/api/src/outbox.rs`, `follow_answer`);
- `Like` and `Announce` name the post, not its author, and inbound reactions are stored;
- a withdrawal derives its id from the relationship (`{actor}#follows/{object}`), so a peer
  can match the `Undo` to the activity it withdraws;
- replies and direct messages are delivered: the note carries its mentions, `to`/`cc`
  follow the visibility, and delivery includes the mentioned inboxes (a direct post goes
  only to them);
- an inbound note is stored with the visibility its addressing states, instead of `public`;
- an ingested note keeps its `tag` (`Mention`, `Hashtag`, `Emoji`) and `attachment`
  list: hashtags and mentions have their own rows, custom emoji are stored in
  `status_emoji`, and media is recorded as the peer's URL, so a remote post renders the
  same shape as a local one (`crates/api/src/activitypub.rs`, `store_note_extras`).

Still open:

- `Update`, `Move`, `Block`, `Flag` and a remote `Delete` are dropped by
  `ingest_activity`'s catch-all;
- no live authorized-fetch peer has confirmed our `Signature`: the request is signed, the
  acceptance is not;
- `server-sdk/crates/federation` is dead code: the server uses
  `nodus-backend/crates/federation`.

## Poll voting

Local polls are live: `GET /api/v1/polls/{id}` returns the poll document, and
`POST /api/v1/polls/{id}/votes` (Mastodon's form-encoded `choices[]`) records one vote per
account per option, so a retried vote never double-counts. A poll on a remote status is
read-only: voting on it is refused with `422` "Validation failed: this poll belongs to
another instance".

An inbound `Question` — or a `Note` whose `attachment` or inline `oneOf` carries one — is
now stored as the same `poll`/`poll_vote` rows local polls use, with `oneOf`/`anyOf`
(single vs multiple), `endTime` mapped to `expires_at`, and the peer's per-option counts
and `votersCount` kept in `poll_tally`/`poll.voters_count`. `GET /api/v1/polls/{id}`
serves it, and a later `Update` replaces those counts instead of adding to them.

Still not implemented: outbound `Question` (compose cannot create polls yet), poll
notifications, and casting a vote here for a poll that lives on another instance.
