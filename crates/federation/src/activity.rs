//! The activities an instance sends and receives.

pub use nodus_activitypub_core::{Activity, ActivityKind, ActivityObject};

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn follow_serializes_with_its_type_tag_and_context() {
        let activity = Activity::new(
            ActivityKind::Follow,
            "https://a.example/users/a",
            "https://b.example/users/b",
        );
        let value = serde_json::to_value(&activity).expect("json");
        assert_eq!(value["type"], "Follow");
        assert_eq!(value["actor"], "https://a.example/users/a");
        assert_eq!(value["object"], "https://b.example/users/b");
        assert!(value["@context"].is_array());
    }

    #[test]
    fn every_activity_names_its_actor() {
        let activities = [
            Activity::new(ActivityKind::Follow, "a", "b"),
            Activity::new(ActivityKind::Like, "a", "b"),
            Activity::new(ActivityKind::Announce, "a", "b"),
            Activity::new(ActivityKind::Block, "a", "b"),
            Activity::new(ActivityKind::Flag, "a", "b"),
            Activity::new(ActivityKind::Move, "a", "b"),
            Activity::new(ActivityKind::Add, "a", "b"),
            Activity::new(ActivityKind::Remove, "a", "b"),
        ];
        for activity in &activities {
            assert_eq!(activity.actor(), "a");
        }
    }

    #[test]
    fn delete_with_embedded_tombstone_deserializes() {
        let raw = json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id": "https://mastodon.social/users/alice/statuses/123/activity",
            "type": "Delete",
            "actor": "https://mastodon.social/users/alice",
            "to": ["https://www.w3.org/ns/activitystreams#Public"],
            "object": {
                "id": "https://mastodon.social/users/alice/statuses/123",
                "type": "Tombstone",
                "atomUri": "https://mastodon.social/users/alice/statuses/123"
            }
        });
        let activity: Activity =
            serde_json::from_value(raw.clone()).expect("deserialize delete with tombstone");
        assert_eq!(activity.kind, ActivityKind::Delete);
        assert_eq!(activity.actor(), "https://mastodon.social/users/alice");
        assert_eq!(
            activity.id.as_deref(),
            Some("https://mastodon.social/users/alice/statuses/123/activity")
        );
        assert_eq!(
            activity.to,
            vec!["https://www.w3.org/ns/activitystreams#Public"]
        );
        assert!(matches!(activity.object, ActivityObject::Value(_)));

        let serialized = serde_json::to_value(&activity).expect("serialize");
        assert_eq!(serialized["type"], "Delete");
        assert_eq!(serialized["object"]["type"], "Tombstone");
        assert!(serialized["@context"].is_array());
    }

    #[test]
    fn delete_with_string_id_deserializes() {
        let raw = json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id": "https://remote.example/activities/delete-1",
            "type": "Delete",
            "actor": "https://remote.example/users/bob",
            "object": "https://remote.example/notes/1"
        });
        let activity: Activity =
            serde_json::from_value(raw).expect("deserialize delete with string");
        assert_eq!(activity.kind, ActivityKind::Delete);
        assert_eq!(activity.actor(), "https://remote.example/users/bob");
        assert_eq!(
            activity.object,
            ActivityObject::Id("https://remote.example/notes/1".to_owned())
        );
    }

    #[test]
    fn activity_preserves_context_id_audience_and_extra_properties() {
        let raw = json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id": "https://remote.example/activities/block-1",
            "type": "Block",
            "actor": "https://remote.example/users/bob",
            "object": "https://remote.example/users/charlie",
            "to": ["https://remote.example/users/charlie"],
            "cc": ["https://remote.example/users/bob/followers"],
            "customExtensionField": "nodus-test-value"
        });
        let activity: Activity = serde_json::from_value(raw).expect("deserialize");
        assert_eq!(activity.kind, ActivityKind::Block);
        assert_eq!(
            activity.id.as_deref(),
            Some("https://remote.example/activities/block-1")
        );
        assert_eq!(activity.to, vec!["https://remote.example/users/charlie"]);
        assert_eq!(
            activity.cc,
            vec!["https://remote.example/users/bob/followers"]
        );
        assert_eq!(
            activity.extra.get("customExtensionField"),
            Some(&json!("nodus-test-value"))
        );

        let serialized = serde_json::to_value(&activity).expect("serialize");
        assert!(serialized.get("@context").is_some());
        assert_eq!(
            serialized["id"],
            "https://remote.example/activities/block-1"
        );
        assert_eq!(serialized["type"], "Block");
        assert_eq!(serialized["actor"], "https://remote.example/users/bob");
        assert_eq!(serialized["object"], "https://remote.example/users/charlie");
        assert_eq!(
            serialized["to"],
            json!(["https://remote.example/users/charlie"])
        );
        assert_eq!(
            serialized["cc"],
            json!(["https://remote.example/users/bob/followers"])
        );
        assert_eq!(serialized["customExtensionField"], "nodus-test-value");
    }

    #[test]
    fn all_kinds_supported_and_round_trip() {
        for kind in [
            ActivityKind::Create,
            ActivityKind::Follow,
            ActivityKind::Accept,
            ActivityKind::Reject,
            ActivityKind::Undo,
            ActivityKind::Delete,
            ActivityKind::Update,
            ActivityKind::Announce,
            ActivityKind::Like,
            ActivityKind::Block,
            ActivityKind::Flag,
            ActivityKind::Move,
            ActivityKind::Add,
            ActivityKind::Remove,
        ] {
            let activity = Activity::new(
                kind,
                "https://a.example/users/a",
                "https://b.example/users/b",
            )
            .with_id("https://a.example/activities/1");
            let value = serde_json::to_value(&activity).expect("serialize");
            assert_eq!(value["type"], kind.as_str());
            let round_trip: Activity = serde_json::from_value(value).expect("deserialize");
            assert_eq!(round_trip.kind, kind);
            assert_eq!(round_trip.actor, "https://a.example/users/a");
            assert_eq!(
                round_trip.id.as_deref(),
                Some("https://a.example/activities/1")
            );
        }
    }

    #[test]
    fn type_identity_with_activitypub_core() {
        let core_activity = nodus_activitypub_core::Activity::new(
            nodus_activitypub_core::ActivityKind::Follow,
            "https://a.example/users/a",
            "https://b.example/users/b",
        );
        let fed_activity: Activity = core_activity;
        assert_eq!(fed_activity.kind, ActivityKind::Follow);
    }
}
