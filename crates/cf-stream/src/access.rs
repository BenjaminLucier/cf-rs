//! Access rules for signed playback tokens.

use serde::Serialize;
use serde_json::{Value, json};

use crate::error::{Error, Result};

/// The most access rules a token may carry.
pub(crate) const MAX_ACCESS_RULES: usize = 5;

/// What an [`AccessRule`] does when it matches.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum AccessAction {
    /// Accept the token.
    Allow,
    /// Reject the token.
    Block,
}

/// A rule restricting where a signed token can be used.
///
/// Rules are evaluated in order and the first match decides: a matching
/// `Block` rule rejects the token. If no rule matches, the token is
/// accepted, so finish an allow-list with [`AccessRule::any`]`(Block)`.
/// A token carries at most 5 rules.
///
/// ```
/// use cf_stream::{AccessAction, AccessRule};
///
/// // Only viewers in the US or Canada.
/// let rules = [
///     AccessRule::country(AccessAction::Allow, ["US", "CA"]),
///     AccessRule::any(AccessAction::Block),
/// ];
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AccessRule {
    kind: Kind,
    action: AccessAction,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Kind {
    Any,
    Ip(Vec<String>),
    Country(Vec<String>),
}

impl AccessRule {
    /// Matches every request.
    ///
    /// ```
    /// let rule = cf_stream::AccessRule::any(cf_stream::AccessAction::Block);
    /// ```
    pub fn any(action: AccessAction) -> Self {
        Self {
            kind: Kind::Any,
            action,
        }
    }

    /// Matches requests from these IP addresses or CIDR ranges. A rule only
    /// matches its own address family, so list IPv4 and IPv6 ranges together.
    ///
    /// ```
    /// use cf_stream::{AccessAction, AccessRule};
    ///
    /// let rule = AccessRule::ip(AccessAction::Allow, ["93.184.216.0/24", "2400:cb00::/32"]);
    /// ```
    pub fn ip<I>(action: AccessAction, ranges: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<String>,
    {
        Self {
            kind: Kind::Ip(ranges.into_iter().map(Into::into).collect()),
            action,
        }
    }

    /// Matches requests from these countries (ISO 3166-1 alpha-2 codes).
    ///
    /// ```
    /// use cf_stream::{AccessAction, AccessRule};
    ///
    /// let rule = AccessRule::country(AccessAction::Block, ["XX"]);
    /// ```
    pub fn country<I>(action: AccessAction, countries: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<String>,
    {
        Self {
            kind: Kind::Country(countries.into_iter().map(Into::into).collect()),
            action,
        }
    }

    /// The rule as sent to Cloudflare, e.g.
    /// `{"type":"ip.geoip.country","action":"allow","country":["US"]}`.
    pub(crate) fn to_json(&self) -> Result<Value> {
        let action = serde_json::to_value(self.action)
            .map_err(|e| Error::InvalidInput(format!("access rule action: {e}")))?;
        Ok(match &self.kind {
            Kind::Any => json!({ "type": "any", "action": action }),
            Kind::Ip(ranges) => {
                non_empty("IP ranges", ranges)?;
                json!({ "type": "ip.src", "action": action, "ip": ranges })
            }
            Kind::Country(countries) => {
                non_empty("countries", countries)?;
                json!({ "type": "ip.geoip.country", "action": action, "country": countries })
            }
        })
    }
}

fn non_empty(what: &str, values: &[String]) -> Result<()> {
    if values.is_empty() || values.iter().any(String::is_empty) {
        return Err(Error::InvalidInput(format!(
            "access rule {what} must be non-empty"
        )));
    }
    Ok(())
}

/// Serializes a token's access rules, enforcing Cloudflare's limit.
pub(crate) fn rules_json(rules: &[AccessRule]) -> Result<Value> {
    if rules.len() > MAX_ACCESS_RULES {
        return Err(Error::InvalidInput(format!(
            "a token can have at most {MAX_ACCESS_RULES} access rules, got {}",
            rules.len()
        )));
    }
    rules
        .iter()
        .map(AccessRule::to_json)
        .collect::<Result<Vec<_>>>()
        .map(Value::Array)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_shapes() {
        let rules = [
            AccessRule::ip(AccessAction::Allow, ["1.2.3.0/24"]),
            AccessRule::country(AccessAction::Allow, ["US", "CA"]),
            AccessRule::any(AccessAction::Block),
        ];
        assert_eq!(
            rules_json(&rules).unwrap(),
            json!([
                {"type": "ip.src", "action": "allow", "ip": ["1.2.3.0/24"]},
                {"type": "ip.geoip.country", "action": "allow", "country": ["US", "CA"]},
                {"type": "any", "action": "block"},
            ])
        );
    }

    #[test]
    fn limits() {
        let six = vec![AccessRule::any(AccessAction::Allow); 6];
        assert!(rules_json(&six).is_err());
        assert!(rules_json(&six[..5]).is_ok());
        let empty: [&str; 0] = [];
        assert!(rules_json(&[AccessRule::ip(AccessAction::Allow, empty)]).is_err());
        assert!(rules_json(&[AccessRule::country(AccessAction::Allow, [""])]).is_err());
    }
}
