//! Fabric's nested JARs are optional candidates, not additional mandatory roots.

use super::{LocalModNode, local_predicate_matches};
use std::collections::{BTreeMap, BTreeSet};

const MAX_NODES: usize = 4096;
const MAX_STATES: usize = 20_000;
const MAX_GROUPS: usize = 256;

pub(super) enum Resolution<'a> {
    Selected(Vec<&'a LocalModNode>),
    Conflict(String),
}

#[derive(Clone)]
struct Choice<'a> {
    node: &'a LocalModNode,
    parents: BTreeSet<&'a str>,
}

pub(super) enum RuntimeCheck {
    NotRuntime,
    Unchecked,
    Matches(bool),
}

type RuntimePredicate<'a> = dyn Fn(&str, &serde_json::Value) -> Result<RuntimeCheck, String> + 'a;

pub(super) fn select<'a>(
    nodes: &[&'a LocalModNode],
    allow_external_missing: bool,
    disabled: &BTreeSet<String>,
    runtime: &RuntimePredicate<'_>,
) -> Result<Resolution<'a>, String> {
    if nodes.len() > MAX_NODES {
        return Err("mod_resolution_limit: local graph nodes".into());
    }
    let mut active = Vec::new();
    let mut nested = BTreeMap::<&str, BTreeMap<&str, Choice<'a>>>::new();
    for &node in nodes {
        if node.depth == 0 {
            active.push(Choice {
                node,
                parents: BTreeSet::new(),
            });
        } else {
            let choice = nested
                .entry(&node.id)
                .or_default()
                .entry(&node.digest)
                .or_insert_with(|| Choice {
                    node,
                    parents: BTreeSet::new(),
                });
            choice.parents.insert(
                node.parent_digest
                    .as_deref()
                    .ok_or("nested Mod parent is missing")?,
            );
        }
    }
    // A directly installed version is mandatory; a nested copy cannot replace it.
    let roots: BTreeSet<_> = active
        .iter()
        .map(|choice| choice.node.id.as_str())
        .collect();
    let mut groups: Vec<_> = nested
        .into_iter()
        .filter(|(id, _)| !roots.contains(id))
        .map(|(_, choices)| {
            let mut choices: Vec<_> = choices.into_values().collect();
            choices.sort_by(|a, b| {
                let order = match (a.node.version.as_deref(), b.node.version.as_deref()) {
                    (Some(a), Some(b)) => {
                        crate::core::modrinth_provider::compare_version_like(b, a)
                            .unwrap_or(0)
                            .cmp(&0)
                    }
                    _ => std::cmp::Ordering::Equal,
                };
                order.then_with(|| a.node.digest.cmp(&b.node.digest))
            });
            choices
        })
        .collect();
    if groups.len() > MAX_GROUPS {
        return Err("mod_resolution_limit: nested Mod identity groups".into());
    }
    let initial = Search {
        groups: &groups,
        allow_external_missing,
        disabled,
        runtime,
        states: 0,
        last_conflict: String::new(),
    };
    // Remove candidates already contradicted by mandatory roots or the runtime,
    // before unrelated optional groups can multiply the search space.
    let mut filtered = Vec::new();
    for group in &groups {
        let mut feasible = Vec::new();
        for choice in group {
            active.push(choice.clone());
            let conflict = initial.conflict(&active, 0);
            active.pop();
            if !matches!(conflict, Ok(Some(_))) {
                feasible.push(choice.clone());
            }
        }
        if !feasible.is_empty() {
            filtered.push(feasible);
        }
    }
    groups = filtered;
    let required: BTreeSet<_> = active.iter().flat_map(|c| c.node.depends.keys()).collect();
    groups.sort_by_key(|group| {
        (
            !group
                .iter()
                .any(|c| c.node.ids.iter().any(|id| required.contains(id))),
            std::cmp::Reverse(group.len()),
            group.iter().map(|c| c.node.depth).min().unwrap_or(0),
            group[0].node.id.as_str(),
        )
    });
    let mut search = Search {
        groups: &groups,
        allow_external_missing,
        disabled,
        runtime,
        states: 0,
        last_conflict: "no compatible nested Mod selection".into(),
    };
    let selected = search.walk(&mut active, 0)?;
    Ok(match selected {
        Some(nodes) => Resolution::Selected(nodes),
        None => Resolution::Conflict(search.last_conflict),
    })
}

struct Search<'a, 's> {
    groups: &'s [Vec<Choice<'a>>],
    allow_external_missing: bool,
    disabled: &'s BTreeSet<String>,
    runtime: &'s RuntimePredicate<'s>,
    states: usize,
    last_conflict: String,
}

impl<'a> Search<'a, '_> {
    fn walk(
        &mut self,
        active: &mut Vec<Choice<'a>>,
        index: usize,
    ) -> Result<Option<Vec<&'a LocalModNode>>, String> {
        self.states += 1;
        if self.states > MAX_STATES {
            return Err("mod_resolution_limit: nested graph search states".into());
        }
        if let Some(conflict) = self.conflict(active, index)? {
            self.last_conflict = conflict;
            return Ok(None);
        }
        if index == self.groups.len() {
            return Ok(Some(active.iter().map(|choice| choice.node).collect()));
        }
        let mut unknown = None;
        for choice in &self.groups[index] {
            active.push(choice.clone());
            let result = self.walk(active, index + 1);
            active.pop();
            match result {
                Ok(Some(nodes)) => return Ok(Some(nodes)),
                Ok(None) => {}
                Err(error) if error.starts_with("mod_resolution_limit:") => return Err(error),
                Err(error) => unknown = Some(error),
            }
        }
        // Nested candidates are IF_POSSIBLE in Fabric, unlike top-level roots.
        match self.walk(active, index + 1) {
            Ok(Some(nodes)) => Ok(Some(nodes)),
            Ok(None) => match unknown {
                Some(error) => Err(error),
                None => Ok(None),
            },
            Err(error) => Err(error),
        }
    }

    fn conflict(&self, active: &[Choice<'a>], index: usize) -> Result<Option<String>, String> {
        let remaining = &self.groups[index..];
        let mut available = BTreeMap::<&str, &LocalModNode>::new();
        let digests: BTreeSet<_> = active.iter().map(|c| c.node.digest.as_str()).collect();
        for choice in active {
            let node = choice.node;
            if !choice.parents.is_empty()
                && !choice.parents.iter().any(|parent| {
                    digests.contains(parent)
                        || remaining.iter().flatten().any(|c| c.node.digest == *parent)
                })
            {
                return Ok(Some(format!(
                    "nested Mod {} has no selected parent",
                    node.id
                )));
            }
            for id in &node.ids {
                if matches!(
                    id.as_str(),
                    "minecraft"
                        | "java"
                        | "fabricloader"
                        | "fabric-loader"
                        | "quilt_loader"
                        | "quilt-loader"
                ) {
                    return Ok(Some("Mod shadows runtime identity".into()));
                }
                if self.disabled.contains(id) {
                    return Ok(Some(format!("required Mod {id} is disabled")));
                }
                if let Some(prior) = available.insert(id, node) {
                    return Ok(Some(format!(
                        "duplicate mod id {id} in {} and {}",
                        prior.filename, node.filename
                    )));
                }
            }
        }
        for choice in active {
            let node = choice.node;
            for (id, predicate) in &node.depends {
                if self.disabled.contains(id) {
                    return Ok(Some(format!("required Mod {id} is disabled")));
                }
                let runtime = (self.runtime)(id, predicate)?;
                if matches!(runtime, RuntimeCheck::Unchecked) {
                    continue;
                }
                if let RuntimeCheck::Matches(matches) = runtime {
                    if !matches {
                        return Ok(Some(format!(
                            "incompatible required dependency {id} for {}",
                            node.filename
                        )));
                    }
                } else if let Some(provider) = available.get(id.as_str()) {
                    if !local_predicate_matches(provider.version.as_deref(), predicate)? {
                        return Ok(Some(format!(
                            "incompatible required dependency {id} for {}",
                            node.filename
                        )));
                    }
                } else {
                    let possible = remaining.iter().flatten().any(|candidate| {
                        candidate.node.ids.contains(id)
                            && local_predicate_matches(candidate.node.version.as_deref(), predicate)
                                .unwrap_or(true)
                    });
                    if !possible && !self.allow_external_missing {
                        return Ok(Some(format!(
                            "unresolved required dependency {id} for {}",
                            node.filename
                        )));
                    }
                }
            }
            for (id, predicate) in &node.incompatible {
                let matches = match (self.runtime)(id, predicate)? {
                    RuntimeCheck::Unchecked => continue,
                    RuntimeCheck::Matches(matches) => matches,
                    RuntimeCheck::NotRuntime => match available.get(id.as_str()) {
                        Some(provider) => {
                            local_predicate_matches(provider.version.as_deref(), predicate)?
                        }
                        None => false,
                    },
                };
                if matches {
                    return Ok(Some(format!(
                        "incompatible dependency {id} declared by {}",
                        node.filename
                    )));
                }
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{mods::collect_local_mods, test_support::zip};

    fn jar(json: &str, children: &[(&str, Vec<u8>)]) -> Vec<u8> {
        let mut value: serde_json::Value = serde_json::from_str(json).unwrap();
        if !children.is_empty() {
            value["jars"] = children
                .iter()
                .map(|(name, _)| serde_json::json!({"file": name}))
                .collect();
        }
        let metadata = serde_json::to_vec(&value).unwrap();
        let mut entries = vec![("fabric.mod.json", metadata.as_slice())];
        entries.extend(
            children
                .iter()
                .map(|(name, bytes)| (*name, bytes.as_slice())),
        );
        zip(&entries, zip::CompressionMethod::Stored)
    }

    fn bundle(id: &str, version: &str, predicate: &str) -> Vec<u8> {
        let library = jar(
            &format!(r#"{{"id":"fabric-api-base","version":"{version}"}}"#),
            &[],
        );
        jar(
            &format!(
                r#"{{"id":"{id}","version":"1","depends":{{"fabric-api-base":"{predicate}"}}}}"#
            ),
            &[("lib.jar", library)],
        )
    }

    fn nodes(jars: &[Vec<u8>]) -> Vec<LocalModNode> {
        let mut nodes = Vec::new();
        for (i, bytes) in jars.iter().enumerate() {
            collect_local_mods(&format!("root-{i}.jar"), bytes, "fabric", 0, &mut nodes).unwrap();
        }
        nodes
    }

    fn resolve(nodes: &[LocalModNode]) -> Resolution<'_> {
        select(
            &nodes.iter().collect::<Vec<_>>(),
            false,
            &BTreeSet::new(),
            &|_, _| Ok(RuntimeCheck::NotRuntime),
        )
        .unwrap()
    }

    #[test]
    fn shared_nested_fabric_api_versions_are_choices_not_duplicate_roots() {
        let nodes = nodes(&[
            bundle("api", "1.0+old", ">=1"),
            bundle("menu", "2.0+new", ">=2"),
        ]);
        let Resolution::Selected(chosen) = resolve(&nodes) else {
            panic!("valid nested selection rejected");
        };
        assert_eq!(chosen.iter().filter(|n| n.depth == 0).count(), 2);
        let selected: Vec<_> = chosen
            .iter()
            .filter(|n| n.id == "fabric-api-base")
            .collect();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].version.as_deref(), Some("2.0+new"));
    }

    #[test]
    fn nested_selection_backtracks_for_ranges_and_deduplicates_identical_bytes() {
        let nodes = nodes(&[
            bundle("one", "1", ">=1 <2"),
            bundle("two", "2", ">=1"),
            bundle("three", "1", ">=1"),
        ]);
        let Resolution::Selected(chosen) = resolve(&nodes) else {
            panic!("compatible older nested version rejected");
        };
        assert_eq!(chosen.len(), 4);
        assert_eq!(
            chosen
                .iter()
                .find(|n| n.id == "fabric-api-base")
                .unwrap()
                .version
                .as_deref(),
            Some("1")
        );
    }

    #[test]
    fn direct_roots_cannot_be_dropped_or_replaced_by_a_nested_version() {
        let direct = jar(r#"{"id":"fabric-api-base","version":"1"}"#, &[]);
        let nodes = nodes(&[direct.clone(), bundle("menu", "2", ">=2")]);
        assert!(matches!(resolve(&nodes), Resolution::Conflict(_)));
        let duplicate = self::nodes(&[direct.clone(), direct]);
        assert!(
            matches!(resolve(&duplicate), Resolution::Conflict(error) if error.contains("duplicate mod id"))
        );
    }

    #[test]
    fn provided_alias_collisions_do_not_discard_the_required_roots() {
        let nested = jar(
            r#"{"id":"optional","version":"1","provides":["mandatory"]}"#,
            &[],
        );
        let nodes = nodes(&[
            jar(r#"{"id":"mandatory","version":"1"}"#, &[]),
            jar(
                r#"{"id":"bundle","version":"1"}"#,
                &[("nested.jar", nested)],
            ),
        ]);
        let Resolution::Selected(chosen) = resolve(&nodes) else {
            panic!("optional alias collision rejected roots");
        };
        assert_eq!(chosen.len(), 2);
        assert!(chosen.iter().all(|n| n.depth == 0));
    }

    #[test]
    fn children_of_an_unselected_parent_are_not_activated() {
        let child = jar(r#"{"id":"child","version":"1"}"#, &[]);
        let old = jar(r#"{"id":"parent","version":"1"}"#, &[("child.jar", child)]);
        let new = jar(r#"{"id":"parent","version":"2"}"#, &[]);
        let nodes = nodes(&[
            jar(
                r#"{"id":"one","version":"1","depends":{"parent":">=2"}}"#,
                &[("parent.jar", old)],
            ),
            jar(r#"{"id":"two","version":"1"}"#, &[("parent.jar", new)]),
        ]);
        let Resolution::Selected(chosen) = resolve(&nodes) else {
            panic!("valid parent selection rejected");
        };
        assert!(chosen.iter().all(|n| n.id != "child"));
        assert_eq!(
            chosen
                .iter()
                .find(|n| n.id == "parent")
                .unwrap()
                .version
                .as_deref(),
            Some("2")
        );
    }

    #[test]
    fn missing_external_dependency_is_allowed_only_during_provider_search() {
        let nodes = nodes(&[jar(
            r#"{"id":"root","version":"1","depends":{"external":">=1"}}"#,
            &[],
        )]);
        assert!(matches!(resolve(&nodes), Resolution::Conflict(_)));
        assert!(matches!(
            select(
                &nodes.iter().collect::<Vec<_>>(),
                true,
                &BTreeSet::new(),
                &|_, _| Ok(RuntimeCheck::NotRuntime)
            )
            .unwrap(),
            Resolution::Selected(_)
        ));
    }

    #[test]
    fn disabled_nested_dependency_cannot_be_reenabled_transitively() {
        let nodes = nodes(&[bundle("root", "1", "*")]);
        assert!(matches!(
            select(
                &nodes.iter().collect::<Vec<_>>(),
                true,
                &BTreeSet::from(["fabric-api-base".into()]),
                &|_, _| Ok(RuntimeCheck::NotRuntime)
            )
            .unwrap(),
            Resolution::Conflict(_)
        ));
    }

    #[test]
    fn runtime_checks_apply_only_to_selected_nested_candidates() {
        let nested = jar(
            r#"{"id":"optional","version":"1","depends":{"java":">=99"}}"#,
            &[],
        );
        let nodes = nodes(&[jar(
            r#"{"id":"root","version":"1"}"#,
            &[("optional.jar", nested)],
        )]);
        let Resolution::Selected(chosen) = select(
            &nodes.iter().collect::<Vec<_>>(),
            false,
            &BTreeSet::new(),
            &|id, _| {
                Ok(if id == "java" {
                    RuntimeCheck::Matches(false)
                } else {
                    RuntimeCheck::NotRuntime
                })
            },
        )
        .unwrap() else {
            panic!("unneeded incompatible nested candidate rejected root");
        };
        assert_eq!(chosen.len(), 1);
    }
}
