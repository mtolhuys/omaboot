//! A deliberately small template renderer.
//!
//! Values are injected as data into `{{PLACEHOLDER}}` slots, never by editing a
//! shipped script in place. A placeholder the caller did not supply is an
//! error, so a template and its generator cannot drift apart silently.

use std::collections::BTreeMap;

use crate::error::{Error, Result};

#[derive(Debug, Default)]
pub struct Values {
    inner: BTreeMap<&'static str, String>,
}

impl Values {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, key: &'static str, value: impl Into<String>) -> &mut Self {
        self.inner.insert(key, value.into());
        self
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.inner.get(key).map(String::as_str)
    }
}

/// Render `template`, then refuse to return a string that still has a slot in it.
pub fn render(file: &str, template: &str, values: &Values) -> Result<String> {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;

    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            // A stray "{{" that never closes is content, not a slot.
            out.push_str(&rest[start..]);
            rest = "";
            break;
        };
        let key = &after[..end];
        if !is_placeholder(key) {
            out.push_str("{{");
            rest = after;
            continue;
        }
        let value = values.get(key).ok_or_else(|| Error::TemplatePlaceholder {
            file: file.to_string(),
            placeholder: key.to_string(),
        })?;
        out.push_str(value);
        rest = &after[end + 2..];
    }
    out.push_str(rest);

    if let Some(leftover) = first_placeholder(&out) {
        return Err(Error::TemplatePlaceholder {
            file: file.to_string(),
            placeholder: leftover,
        });
    }
    Ok(out)
}

fn is_placeholder(key: &str) -> bool {
    !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

fn first_placeholder(text: &str) -> Option<String> {
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let end = after.find("}}")?;
        let key = &after[..end];
        if is_placeholder(key) {
            return Some(key.to_string());
        }
        rest = after;
    }
    None
}

/// Escape a string for a double-quoted Plymouth script literal.
///
/// Control characters are refused rather than escaped: the validator already
/// rejects them, and a boot screen is the wrong place to discover that a
/// newline made it into a message.
pub fn plymouth_string(value: &str, target: &str) -> Result<String> {
    reject_control(value, target)?;
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            other => out.push(other),
        }
    }
    Ok(out)
}

/// Escape a string for a double-quoted QML literal.
pub fn qml_string(value: &str, target: &str) -> Result<String> {
    reject_control(value, target)?;
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            other => out.push(other),
        }
    }
    Ok(out)
}

/// Escape a value for a freedesktop `key=value` line, where a newline would
/// start a new key and a trailing backslash would continue the line.
pub fn desktop_value(value: &str, target: &str) -> Result<String> {
    reject_control(value, target)?;
    Ok(value.replace('\\', "\\\\"))
}

fn reject_control(value: &str, target: &str) -> Result<()> {
    if let Some(bad) = value.chars().find(|c| c.is_control()) {
        return Err(Error::Unrepresentable {
            value: value.to_string(),
            target: target.to_string(),
            what: format!("the control character {bad:?}"),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitutes_values() {
        let mut values = Values::new();
        values.set("NAME", "Nord").set("COUNT", "3");
        let out = render("t", "a {{NAME}} b {{COUNT}}", &values).unwrap();
        assert_eq!(out, "a Nord b 3");
    }

    #[test]
    fn missing_value_is_an_error_naming_the_placeholder() {
        let values = Values::new();
        let error = render("omaboot.script", "x {{MISSING}}", &values).unwrap_err();
        assert!(error.to_string().contains("MISSING"), "{error}");
        assert!(error.to_string().contains("omaboot.script"), "{error}");
    }

    #[test]
    fn a_value_that_injects_a_placeholder_is_caught() {
        // A theme cannot smuggle a slot in through its own content.
        let mut values = Values::new();
        values.set("NAME", "{{OTHER}}");
        let error = render("t", "{{NAME}}", &values).unwrap_err();
        assert!(error.to_string().contains("OTHER"), "{error}");
    }

    #[test]
    fn braces_that_are_not_placeholders_survive() {
        let values = Values::new();
        assert_eq!(
            render("t", "fun x() {\n}\n", &values).unwrap(),
            "fun x() {\n}\n"
        );
        assert_eq!(
            render("t", "a {{lowercase}} b", &values).unwrap(),
            "a {{lowercase}} b"
        );
        assert_eq!(render("t", "unclosed {{", &values).unwrap(), "unclosed {{");
    }

    #[test]
    fn quotes_and_backslashes_are_escaped() {
        assert_eq!(
            plymouth_string("say \"hi\\bye\"", "unlock.message").unwrap(),
            "say \\\"hi\\\\bye\\\""
        );
        assert_eq!(qml_string("a\"b", "meta.name").unwrap(), "a\\\"b");
    }

    #[test]
    fn control_characters_are_refused_not_escaped() {
        let error = plymouth_string("two\nlines", "shutdown.message").unwrap_err();
        assert!(error.to_string().contains("shutdown.message"), "{error}");
        assert!(desktop_value("a\nName=b", "meta.name").is_err());
    }
}
