//! Bounded feasibility search, not a proof of minimum sacrifice or compatibility.

use std::collections::{BTreeSet, VecDeque};

pub(super) enum Trial<T> {
    Feasible(T),
    Conflict,
}

/// Keep as many root groups as observed feasible. Unknown failures abort the
/// search; they must never masquerade as evidence to remove another root.
pub(super) async fn find<T, F, Fut>(
    roots: &[String],
    budget: usize,
    mut trial: F,
) -> Result<Option<(Vec<String>, T)>, String>
where
    F: FnMut(Vec<String>) -> Fut,
    Fut: std::future::Future<Output = Result<Trial<T>, String>>,
{
    if roots.is_empty() || roots.len() > 128 || budget < 2 {
        return Ok(None);
    }
    let all: BTreeSet<_> = roots.iter().cloned().collect();
    if all.len() != roots.len() {
        return Err("duplicate alternative Mod root".into());
    }
    let mut omitted = all.clone();
    let mut best = match trial(omitted.iter().cloned().collect()).await? {
        Trial::Feasible(value) => value,
        Trial::Conflict => return Ok(None),
    };
    let mut groups = VecDeque::new();
    // The complete set already failed before entering this search.
    let middle = roots.len().div_ceil(2);
    groups.push_back(roots[..middle].to_vec());
    if middle < roots.len() {
        groups.push_back(roots[middle..].to_vec());
    }
    let mut attempts = 1;
    while let Some(group) = groups.pop_front() {
        if attempts >= budget || group.is_empty() {
            break;
        }
        let candidate: BTreeSet<_> = omitted
            .iter()
            .filter(|key| !group.contains(key))
            .cloned()
            .collect();
        if candidate.is_empty() {
            continue;
        }
        attempts += 1;
        match trial(candidate.iter().cloned().collect()).await? {
            Trial::Feasible(value) => {
                omitted = candidate;
                best = value;
            }
            Trial::Conflict if group.len() > 1 => {
                let middle = group.len().div_ceil(2);
                groups.push_back(group[..middle].to_vec());
                groups.push_back(group[middle..].to_vec());
            }
            Trial::Conflict => {}
        }
    }
    Ok(Some((omitted.into_iter().collect(), best)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn verified_groups_preserve_available_roots_and_list_every_omission() -> Result<(), String>
    {
        let roots = ["renderer", "shader", "utility", "menu"].map(str::to_owned);
        let result = find(&roots, 16, |omitted| async move {
            if omitted.contains(&"renderer".into()) && omitted.contains(&"shader".into()) {
                Ok(Trial::Feasible(omitted.clone()))
            } else {
                Ok(Trial::Conflict)
            }
        })
        .await?
        .ok_or("expected a feasible alternative")?;
        assert_eq!(result.0, ["renderer", "shader"]);
        assert_eq!(result.0, result.1);
        Ok(())
    }

    #[tokio::test]
    async fn unknown_failure_never_returns_an_earlier_smaller_success() {
        let mut count = 0;
        let result = find(&["a".into(), "b".into()], 8, |_| {
            count += 1;
            let current = count;
            async move {
                if current == 1 {
                    Ok(Trial::Feasible(()))
                } else {
                    Err("provider unavailable".into())
                }
            }
        })
        .await;
        assert_eq!(result.err().as_deref(), Some("provider unavailable"));
    }

    #[tokio::test]
    async fn bounded_search_only_returns_a_configuration_actually_verified() -> Result<(), String> {
        let mut attempts = 0;
        let result = find(&["a".into(), "b".into(), "c".into()], 2, |omitted| {
            attempts += 1;
            async move { Ok(Trial::Feasible(omitted)) }
        })
        .await?
        .ok_or("expected feasible result")?;
        assert_eq!(attempts, 2);
        assert_eq!(result.0, result.1);
        assert!(!result.0.is_empty());
        Ok(())
    }
}
