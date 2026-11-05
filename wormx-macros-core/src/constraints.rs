use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use wormx_core::TableRef;

pub type BoxDynError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Constraints {
    #[serde(default)]
    pub primary_key: Vec<String>,
    #[serde(default)]
    pub defaults: BTreeMap<String, String>,
    #[serde(default)]
    pub identity: Vec<String>,
    #[serde(default)]
    pub generated: Vec<String>,
    #[serde(default)]
    pub foreign_keys: Vec<ForeignKey>,
    #[serde(default)]
    pub referenced_by: Vec<IncomingForeignKey>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ForeignKey {
    #[serde(default)]
    pub name: Option<String>,
    pub columns: Vec<String>,
    pub referenced_table: TableRef,
    pub referenced_columns: Vec<String>,
    #[serde(default)]
    pub columns_nullable: Vec<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IncomingForeignKey {
    pub source_table: TableRef,
    pub columns: Vec<String>,
    pub referenced_columns: Vec<String>,
    #[serde(default)]
    pub columns_nullable: Vec<bool>,
}

impl Constraints {
    pub fn is_primary_key(&self, column: &str) -> bool {
        self.primary_key.iter().any(|name| name == column)
    }

    pub fn has_default(&self, column: &str) -> bool {
        self.defaults.contains_key(column)
    }

    pub fn is_identity(&self, column: &str) -> bool {
        self.identity.iter().any(|name| name == column)
    }

    pub fn is_generated(&self, column: &str) -> bool {
        self.generated.iter().any(|name| name == column)
    }
}
