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
    KindInTwoSchemas {
        kind: String,
        first: String,
        second: String,
    },
    PrefixInTwoSchemas {
        prefix: String,
        first: String,
        second: String,
        kind: String,
        fix: Option<String>,
    },
    NameIsShortId {
        name: String,
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
            Self::KindInTwoSchemas { kind, first, second } => write!(
                f,
                "{first}.toml and {second}.toml both declare node kind {kind:?}; a kind belongs \
                 to one map"
            ),
            Self::PrefixInTwoSchemas {
                prefix,
                first,
                second,
                kind,
                fix,
            } => {
                write!(
                    f,
                    "{second}.toml's {kind:?} takes the short id prefix {prefix:?}, which \
                     {first}.toml already gives"
                )?;
                match fix {
                    Some(fix) => write!(f, "; set prefix = {fix:?} on {kind:?}"),
                    None => write!(f, "; set a prefix on {kind:?}"),
                }
            }
            Self::NameIsShortId { name } => write!(
                f,
                "{name}.toml is named like a short id; rename the file so `show {name}` names \
                 the map, not a node"
            ),
        }
    }
}

impl std::error::Error for SchemaError {}
