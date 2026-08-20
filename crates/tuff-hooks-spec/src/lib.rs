//! Canonical hook metadata for Tuff-standard hooks.
//!
//! This crate models the stable vocabulary Tuff uses when a hook is authored in
//! Tuff's manifest format and then rendered by an adapter into a native harness
//! format. It does not model native harness hook fragments; those remain
//! adapter-owned passthrough data.

use serde::{Deserialize, Serialize};

/// Current version of the Tuff hook specification.
pub const SPEC_VERSION: &str = "0.1.0";

/// Canonical Tuff hook events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookEvent {
    /// A session or subagent starts.
    SessionStart,
    /// A main session ends.
    SessionEnd,
    /// A harness is about to execute a tool.
    PreToolUse,
    /// A harness has finished executing a tool.
    PostToolUse,
    /// A harness is about to finish the current turn or task.
    BeforeFinish,
    /// A harness saved a file.
    AfterSave,
    /// A harness stop/continuation point.
    Stop,
}

impl HookEvent {
    /// Returns the canonical snake_case event name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SessionStart => "session_start",
            Self::SessionEnd => "session_end",
            Self::PreToolUse => "pre_tool_use",
            Self::PostToolUse => "post_tool_use",
            Self::BeforeFinish => "before_finish",
            Self::AfterSave => "after_save",
            Self::Stop => "stop",
        }
    }
}

impl std::fmt::Display for HookEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Payload type names used by [`PayloadField`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PayloadValueType {
    /// String value.
    String,
    /// Boolean value.
    Boolean,
    /// Number value.
    Number,
    /// Structured JSON object.
    Object,
    /// JSON array.
    Array,
}

/// A field expected in a hook payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PayloadField {
    /// Field name as it appears in the canonical payload descriptor.
    pub name: &'static str,
    /// Field value type.
    pub value_type: PayloadValueType,
    /// Whether the field is required for the canonical event.
    pub required: bool,
    /// Human-facing field description.
    pub description: &'static str,
}

/// A structured descriptor for a canonical event payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PayloadSchema {
    /// Common and event-specific fields.
    pub fields: &'static [PayloadField],
}

impl PayloadSchema {
    /// Returns true when the descriptor contains no declared fields.
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }
}

/// What a hook can block when a handler returns a blocking result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockingScope {
    /// The event is not blocking.
    NotBlocking,
    /// The hook can block the action that has not yet run.
    BlocksAction,
    /// The hook can block continuation after an action or turn.
    BlocksContinuation,
    /// Harness-specific blocking semantics documented by the adapter.
    Custom(&'static str),
}

/// Canonical event metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HookEventSpec {
    /// Canonical typed event.
    pub event: HookEvent,
    /// Event name used in Tuff-standard manifests.
    pub canonical_name: &'static str,
    /// Blocking behavior exposed by the canonical event.
    pub blocking: BlockingScope,
    /// First Tuff hook spec version that includes this event.
    pub since_spec_version: &'static str,
    /// Structured payload descriptor.
    pub payload_schema: PayloadSchema,
}

/// Adapter support level for a canonical event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CoverageLevel {
    /// The adapter supports the event for the declared scope.
    Full,
    /// The adapter supports the event only for the declared scope.
    Partial,
    /// The adapter does not support the event.
    Unsupported,
}

impl CoverageLevel {
    /// Returns whether this coverage level can render/run a hook.
    pub const fn is_supported(self) -> bool {
        !matches!(self, Self::Unsupported)
    }
}

/// Compatibility for one canonical event on one adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CompatibilityEntry {
    /// Canonical Tuff hook event.
    pub event: HookEvent,
    /// Native harness event name emitted by the adapter, when supported.
    pub native_event: Option<&'static str>,
    /// Legacy or native names accepted as aliases in Tuff manifests.
    pub aliases: &'static [&'static str],
    /// Support level for this event.
    pub coverage: CoverageLevel,
    /// Scope labels that explain partial coverage.
    pub scope: &'static [&'static str],
    /// Adapter or harness caveat, if any.
    pub caveat: Option<&'static str>,
    /// Source note or URL for the compatibility row.
    pub source: Option<&'static str>,
    /// First harness version known to have this behavior.
    pub since_harness_version: Option<&'static str>,
    /// Last harness version known to have this behavior.
    pub until_harness_version: Option<&'static str>,
}

impl CompatibilityEntry {
    /// Returns true when this row matches a manifest event name.
    pub fn matches_name(&self, raw_event: &str) -> bool {
        self.event.as_str() == raw_event || self.aliases.contains(&raw_event)
    }

    /// Returns the native event to emit when this row is supported.
    pub fn native_event_name(&self) -> Option<&'static str> {
        self.coverage
            .is_supported()
            .then_some(self.native_event)
            .flatten()
    }
}

/// Static adapter compatibility data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CompatibilityMatrix {
    /// Tuff hook spec version this matrix targets.
    pub spec_version: &'static str,
    /// Adapter id, such as `open-agents` or `claude`.
    pub adapter: &'static str,
    /// Matrix rows.
    pub events: &'static [CompatibilityEntry],
}

impl CompatibilityMatrix {
    /// Finds the compatibility row for a manifest event name or alias.
    pub fn find_event(&self, raw_event: &str) -> Option<&CompatibilityEntry> {
        self.events
            .iter()
            .find(|entry| entry.event.as_str() == raw_event)
            .or_else(|| {
                self.events
                    .iter()
                    .find(|entry| entry.aliases.contains(&raw_event))
            })
    }

    /// Returns supported native event names for user-facing error messages.
    pub fn supported_native_events(&self) -> Vec<&'static str> {
        self.events
            .iter()
            .filter_map(CompatibilityEntry::native_event_name)
            .collect()
    }
}

/// Common payload fields available to most Tuff-standard hooks.
pub const COMMON_PAYLOAD_FIELDS: &[PayloadField] = &[
    PayloadField {
        name: "session_id",
        value_type: PayloadValueType::String,
        required: false,
        description: "Harness session identifier, when available.",
    },
    PayloadField {
        name: "cwd",
        value_type: PayloadValueType::String,
        required: false,
        description: "Working directory for the hook invocation.",
    },
];

/// Current canonical event metadata.
pub const EVENT_SPECS: &[HookEventSpec] = &[
    HookEventSpec {
        event: HookEvent::SessionStart,
        canonical_name: "session_start",
        blocking: BlockingScope::NotBlocking,
        since_spec_version: SPEC_VERSION,
        payload_schema: PayloadSchema {
            fields: COMMON_PAYLOAD_FIELDS,
        },
    },
    HookEventSpec {
        event: HookEvent::SessionEnd,
        canonical_name: "session_end",
        blocking: BlockingScope::NotBlocking,
        since_spec_version: SPEC_VERSION,
        payload_schema: PayloadSchema {
            fields: COMMON_PAYLOAD_FIELDS,
        },
    },
    HookEventSpec {
        event: HookEvent::PreToolUse,
        canonical_name: "pre_tool_use",
        blocking: BlockingScope::BlocksAction,
        since_spec_version: SPEC_VERSION,
        payload_schema: PayloadSchema {
            fields: COMMON_PAYLOAD_FIELDS,
        },
    },
    HookEventSpec {
        event: HookEvent::PostToolUse,
        canonical_name: "post_tool_use",
        blocking: BlockingScope::BlocksContinuation,
        since_spec_version: SPEC_VERSION,
        payload_schema: PayloadSchema {
            fields: COMMON_PAYLOAD_FIELDS,
        },
    },
    HookEventSpec {
        event: HookEvent::BeforeFinish,
        canonical_name: "before_finish",
        blocking: BlockingScope::BlocksContinuation,
        since_spec_version: SPEC_VERSION,
        payload_schema: PayloadSchema {
            fields: COMMON_PAYLOAD_FIELDS,
        },
    },
    HookEventSpec {
        event: HookEvent::AfterSave,
        canonical_name: "after_save",
        blocking: BlockingScope::NotBlocking,
        since_spec_version: SPEC_VERSION,
        payload_schema: PayloadSchema {
            fields: COMMON_PAYLOAD_FIELDS,
        },
    },
    HookEventSpec {
        event: HookEvent::Stop,
        canonical_name: "stop",
        blocking: BlockingScope::BlocksContinuation,
        since_spec_version: SPEC_VERSION,
        payload_schema: PayloadSchema {
            fields: COMMON_PAYLOAD_FIELDS,
        },
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    const ENTRY: CompatibilityEntry = CompatibilityEntry {
        event: HookEvent::PreToolUse,
        native_event: Some("PreToolUse"),
        aliases: &["pre_tool_execution"],
        coverage: CoverageLevel::Full,
        scope: &["Bash"],
        caveat: None,
        source: None,
        since_harness_version: None,
        until_harness_version: None,
    };

    #[test]
    fn hook_event_display_uses_canonical_name() {
        assert_eq!(HookEvent::PreToolUse.to_string(), "pre_tool_use");
    }

    #[test]
    fn compatibility_entry_matches_canonical_name_and_alias() {
        assert!(ENTRY.matches_name("pre_tool_use"));
        assert!(ENTRY.matches_name("pre_tool_execution"));
        assert!(!ENTRY.matches_name("before_finish"));
    }

    #[test]
    fn unsupported_entries_have_no_native_event_name() {
        let entry = CompatibilityEntry {
            coverage: CoverageLevel::Unsupported,
            ..ENTRY
        };

        assert_eq!(entry.native_event_name(), None);
    }

    #[test]
    fn canonical_name_takes_precedence_over_an_earlier_alias() {
        const EVENTS: &[CompatibilityEntry] = &[
            CompatibilityEntry {
                event: HookEvent::BeforeFinish,
                native_event: Some("stop"),
                aliases: &["stop"],
                coverage: CoverageLevel::Partial,
                scope: &[],
                caveat: None,
                source: None,
                since_harness_version: None,
                until_harness_version: None,
            },
            CompatibilityEntry {
                event: HookEvent::Stop,
                native_event: Some("stop"),
                aliases: &[],
                coverage: CoverageLevel::Full,
                scope: &[],
                caveat: None,
                source: None,
                since_harness_version: None,
                until_harness_version: None,
            },
        ];
        let matrix = CompatibilityMatrix {
            spec_version: SPEC_VERSION,
            adapter: "test",
            events: EVENTS,
        };

        let matched = matrix.find_event("stop").expect("stop event");
        assert_eq!(matched.event, HookEvent::Stop);
        assert_eq!(matched.coverage, CoverageLevel::Full);
    }

    #[test]
    fn aliases_remain_available_when_no_canonical_name_matches() {
        const EVENTS: &[CompatibilityEntry] = &[CompatibilityEntry {
            event: HookEvent::PreToolUse,
            native_event: Some("PreToolUse"),
            aliases: &["BeforeTool"],
            coverage: CoverageLevel::Full,
            scope: &[],
            caveat: None,
            source: None,
            since_harness_version: None,
            until_harness_version: None,
        }];
        let matrix = CompatibilityMatrix {
            spec_version: SPEC_VERSION,
            adapter: "test",
            events: EVENTS,
        };

        assert_eq!(
            matrix.find_event("BeforeTool").map(|entry| entry.event),
            Some(HookEvent::PreToolUse)
        );
    }
}
