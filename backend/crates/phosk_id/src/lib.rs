//! `phosk_id` — typed id newtypes (ADR-008 identity, L0 foundation).
//!
//! Every domain entity carries BOTH a typed id (a newtype over [`uuid::Uuid`],
//! defined here) AND a human `name`/`label` (a `String`, on the entity). The id
//! is the **relation key**: entities reference one another by id, never by name.
//! The name is what the UI and the AI show; it is presentation, not identity.
//!
//! Each newtype is `Copy + Clone + PartialEq + Eq + Hash + Debug`, serialises
//! **transparently** (`#[serde(transparent)]`) as the underlying UUID, and
//! exposes a uniform surface: [`new()`](ReceiptId::new) (a fresh random v4),
//! `from_uuid(Uuid)`, `as_uuid() -> Uuid`, and `Display` (the hyphenated UUID).
//!
//! Ids are **opaque inside the domain** — feature code moves them around but
//! never inspects their bytes. At the wire edge a service formats
//! `id.as_uuid().to_string()` (or substitutes a stable seed slug); the typed id
//! never leaks its concrete representation into a DTO field type.
//!
//! The [`id_newtype!`] macro generates each type so the contract stays uniform.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Define a transparent typed-id newtype over [`uuid::Uuid`].
///
/// Generates the struct, its uniform constructor/accessor surface
/// (`new`/`from_uuid`/`as_uuid`), a [`Default`] impl (a fresh random id), a
/// `Display` of the hyphenated UUID, and a `From<Uuid>` conversion. The id
/// serialises transparently as its inner UUID (no wrapper object on the wire).
#[macro_export]
macro_rules! id_newtype {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            /// A fresh, random (v4) id. The normal way to mint a new entity id.
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            /// Wrap an existing [`Uuid`] (e.g. one rehydrated from storage).
            #[must_use]
            pub const fn from_uuid(uuid: Uuid) -> Self {
                Self(uuid)
            }

            /// The underlying [`Uuid`]. Use this to format the id at the edge
            /// (`id.as_uuid().to_string()`); the domain otherwise treats the id
            /// as opaque.
            #[must_use]
            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl From<Uuid> for $name {
            fn from(uuid: Uuid) -> Self {
                Self(uuid)
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                ::std::fmt::Display::fmt(&self.0, f)
            }
        }
    };
}

id_newtype!(
    /// Identity of a [`Receipt`](../phosk_model) — one captured spend document.
    ReceiptId
);
id_newtype!(
    /// Identity of a `LineItem` — one itemised row on a receipt.
    LineItemId
);
id_newtype!(
    /// Identity of a `CategoryCap` / spending channel.
    CategoryId
);
id_newtype!(
    /// Identity of a `Budget` (the cycle-level config).
    BudgetId
);
id_newtype!(
    /// Identity of a `Subscription` (a recurring charge).
    SubscriptionId
);
id_newtype!(
    /// Identity of a `Charge` (one recorded billing event of a subscription).
    ChargeId
);
id_newtype!(
    /// Identity of a `Debt` (institutional obligation).
    DebtId
);
id_newtype!(
    /// Identity of a `DebtPayment`.
    PaymentId
);
id_newtype!(
    /// Identity of a `PersonalIou` (an informal person-to-person debt).
    PersonalIouId
);
id_newtype!(
    /// Identity of a `Signal` (a tracked item-signal, e.g. Coffee).
    SignalId
);
id_newtype!(
    /// Identity of an `Alert`.
    AlertId
);
id_newtype!(
    /// Identity of a `Chat` (an AI conversation thread).
    ChatId
);
id_newtype!(
    /// Identity of a `Message` within a chat.
    MessageId
);
id_newtype!(
    /// Identity of an `AiSuggestion`.
    SuggestionId
);
id_newtype!(
    /// Identity of a `FeedItem` (an AI activity-feed entry).
    FeedItemId
);
id_newtype!(
    /// Identity of a `CorrectionEvent` (an audit-log entry).
    CorrectionId
);
id_newtype!(
    /// Identity of a `BudgetChange` (a global-budget history entry).
    BudgetChangeId
);
id_newtype!(
    /// Identity of a `Preference` (a user setting).
    PreferenceId
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_ids_are_distinct() {
        assert_ne!(ReceiptId::new(), ReceiptId::new());
    }

    #[test]
    fn from_uuid_and_as_uuid_round_trip() {
        let u = Uuid::new_v4();
        let id = LineItemId::from_uuid(u);
        assert_eq!(id.as_uuid(), u);
    }

    #[test]
    fn equal_when_inner_uuid_equal() {
        let u = Uuid::new_v4();
        assert_eq!(CategoryId::from_uuid(u), CategoryId::from_uuid(u));
    }

    #[test]
    fn display_is_the_hyphenated_uuid() {
        let u = Uuid::new_v4();
        let id = DebtId::from_uuid(u);
        assert_eq!(id.to_string(), u.to_string());
    }

    #[test]
    fn serialises_transparently_as_the_uuid_string() {
        let u = Uuid::new_v4();
        let id = SubscriptionId::from_uuid(u);
        let value: serde_json::Value = serde_json::to_value(id).expect("to value");
        // No wrapper object — the id IS its uuid string on the wire.
        assert_eq!(value, serde_json::Value::String(u.to_string()));
    }

    #[test]
    fn round_trips_through_json() {
        let id = SignalId::new();
        let json = serde_json::to_string(&id).expect("serialize");
        let back: SignalId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(id, back);
    }

    #[test]
    fn ids_are_hashable_for_use_as_map_keys() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        let id = AlertId::new();
        set.insert(id);
        assert!(set.contains(&id));
    }
}
