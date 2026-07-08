use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AdmonitionKind {
    Attention,
    Caution,
    Danger,
    Error,
    Hint,
    Important,
    Note,
    Tip,
    Warning,
    Admonition,
}

impl AdmonitionKind {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Attention => "attention",
            Self::Caution => "caution",
            Self::Danger => "danger",
            Self::Error => "error",
            Self::Hint => "hint",
            Self::Important => "important",
            Self::Note => "note",
            Self::Tip => "tip",
            Self::Warning => "warning",
            Self::Admonition => "admonition",
        }
    }
}

impl std::str::FromStr for AdmonitionKind {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "attention" => Ok(Self::Attention),
            "caution" => Ok(Self::Caution),
            "danger" => Ok(Self::Danger),
            "error" => Ok(Self::Error),
            "hint" => Ok(Self::Hint),
            "important" => Ok(Self::Important),
            "note" => Ok(Self::Note),
            "tip" => Ok(Self::Tip),
            "warning" => Ok(Self::Warning),
            "admonition" => Ok(Self::Admonition),
            _ => Err(()),
        }
    }
}

impl std::fmt::Display for AdmonitionKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
