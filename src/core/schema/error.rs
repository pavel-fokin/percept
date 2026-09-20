use std::fmt;

/// Why a schema declaration does not describe a valid map kind.
#[derive(Debug, PartialEq, Eq)]
pub enum SchemaError {
    NoNodeKinds,
    BlankKind {
        group: &'static str,
    },
    BlankProperty {
        kind: String,
    },
    PrefixCollision {
        first: String,
        second: String,
        prefix: String,
    },
    SecondClosedList {
        kind: String,
        property: String,
        first: String,
    },
    TooFewValues {
        kind: String,
        property: String,
    },
    BlankValue {
        kind: String,
        property: String,
    },
    RepeatedValue {
        kind: String,
        property: String,
        value: String,
    },
    EmptyEdgeEnd {
        edge: String,
        end: &'static str,
    },
    UnknownEdgeEnd {
        edge: String,
        end: &'static str,
        kind: String,
    },
}

impl fmt::Display for SchemaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoNodeKinds => write!(f, "declares no node kinds"),
            Self::BlankKind { group } => write!(f, "a {group} kind must not be blank"),
            Self::BlankProperty { kind } => {
                write!(f, "node kind {kind:?} declares a blank property")
            }
            Self::PrefixCollision {
                first,
                second,
                prefix,
            } => write!(
                f,
                "node kinds {first:?} and {second:?} both take the short id prefix {prefix:?}"
            ),
            Self::SecondClosedList {
                kind,
                property,
                first,
            } => write!(
                f,
                "node kind {kind:?} declares a second closed list, {property:?}; a kind carries at \
                 most one, alongside {first:?}"
            ),
            Self::TooFewValues { kind, property } => write!(
                f,
                "node kind {kind:?} declares fewer than two values for {property:?}"
            ),
            Self::BlankValue { kind, property } => {
                write!(
                    f,
                    "node kind {kind:?} declares a blank value for {property:?}"
                )
            }
            Self::RepeatedValue {
                kind,
                property,
                value,
            } => write!(
                f,
                "node kind {kind:?} declares the value {value:?} twice for {property:?}"
            ),
            Self::EmptyEdgeEnd { edge, end } => {
                write!(f, "edge kind {edge:?}'s {end} names no node kind")
            }
            Self::UnknownEdgeEnd { edge, end, kind } => write!(
                f,
                "edge kind {edge:?}'s {end} names {kind:?}, which is not a declared node kind"
            ),
        }
    }
}

impl std::error::Error for SchemaError {}
