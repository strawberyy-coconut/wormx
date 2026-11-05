//! Identifier-to-Rust naming helpers.

use heck::{ToSnakeCase, ToUpperCamelCase};

/// Rust keywords that can be used as raw identifiers.
const RAW_KEYWORDS: &[&str] = &[
    "as", "break", "const", "continue", "else", "enum", "extern", "false", "fn", "for", "if",
    "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return", "static",
    "struct", "trait", "true", "type", "unsafe", "use", "where", "while", "async", "await", "dyn",
    "abstract", "become", "box", "do", "final", "macro", "override", "priv", "typeof", "unsized",
    "virtual", "yield", "try", "gen",
];

/// Keywords that cannot be raw identifiers.
const NON_RAW_KEYWORDS: &[&str] = &["crate", "self", "Self", "super", "_"];

pub fn struct_name(table_name: &str) -> String {
    let mut name = table_name.to_upper_camel_case();
    if name.is_empty() {
        name = "Table".to_owned();
    }

    if name.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        name.insert(0, '_');
    }

    if NON_RAW_KEYWORDS.contains(&name.as_str()) {
        name.push('_');
    } else if RAW_KEYWORDS.contains(&name.as_str()) {
        name = format!("r#{name}");
    }

    name
}

pub fn field_name(column_name: &str) -> String {
    let mut name = if is_plain_identifier(column_name) {
        column_name.to_owned()
    } else {
        column_name.to_snake_case()
    };

    if name.is_empty() || name == "_" {
        name = "column".to_owned();
    }

    if name.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        name.insert(0, '_');
    }

    if NON_RAW_KEYWORDS.contains(&name.as_str()) {
        name.push('_');
    } else if RAW_KEYWORDS.contains(&name.as_str()) {
        name = format!("r#{name}");
    }

    name
}

fn is_plain_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    if !(first == '_' || first.is_ascii_alphabetic()) {
        return false;
    }

    chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn struct_names() {
        assert_eq!(struct_name("users"), "Users");
        assert_eq!(struct_name("user_profiles"), "UserProfiles");
        assert_eq!(struct_name("123_users"), "_123Users");
        assert_eq!(struct_name("type"), "Type");
    }

    #[test]
    fn field_names() {
        assert_eq!(field_name("user_id"), "user_id");
        assert_eq!(field_name("User ID"), "user_id");
        assert_eq!(field_name("type"), "r#type");
        assert_eq!(field_name("self"), "self_");
        assert_eq!(field_name("123"), "_123");
    }
}
