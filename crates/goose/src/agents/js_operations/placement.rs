use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::{bail, Result};

use super::JavaScriptOperation;
use crate::agents::state_machine::{GooseEffect, Operation, Step};
use crate::session::Session;

#[derive(Debug, PartialEq, Eq)]
pub enum OperationPlacement {
    Before(String),
    After(String),
    Replace(String),
}

pub struct ConfiguredOperation {
    pub operation: Arc<JavaScriptOperation>,
    pub placement: OperationPlacement,
}

struct Node<'a> {
    step: Option<Step<'a, Session, GooseEffect>>,
    name: String,
    before: Vec<usize>,
    after: Vec<usize>,
}

fn step_name<'a>(step: &'a Step<'_, Session, GooseEffect>) -> &'a str {
    match step {
        Step::Operation(operation) => operation.name(),
        Step::Inference(inference) => inference.name(),
    }
}

fn append_group<'a>(
    index: usize,
    nodes: &mut [Node<'a>],
    ordered: &mut Vec<Step<'a, Session, GooseEffect>>,
) {
    for child in std::mem::take(&mut nodes[index].before) {
        append_group(child, nodes, ordered);
    }
    ordered.push(nodes[index].step.take().expect("operation placed once"));
    for child in std::mem::take(&mut nodes[index].after) {
        append_group(child, nodes, ordered);
    }
}

/// Replaces built-in Operations and inserts JavaScript Operations beside their named anchors.
pub fn place_operations<'a>(
    steps: Vec<Step<'a, Session, GooseEffect>>,
    operations: Vec<ConfiguredOperation>,
) -> Result<Vec<Step<'a, Session, GooseEffect>>> {
    let roots = steps.len();
    let mut nodes: Vec<_> = steps
        .into_iter()
        .map(|step| Node {
            name: step_name(&step).to_owned(),
            step: Some(step),
            before: Vec::new(),
            after: Vec::new(),
        })
        .collect();
    let mut names = HashMap::new();
    for (index, node) in nodes.iter().enumerate() {
        if names.insert(node.name.clone(), index).is_some() {
            bail!("Duplicate operation name: {}", node.name);
        }
    }
    let builtins = names.clone();
    let mut replaced = HashSet::new();
    let mut insertions = Vec::new();
    for operation in operations {
        let OperationPlacement::Replace(target) = &operation.placement else {
            insertions.push(operation);
            continue;
        };
        let name = operation.operation.name().to_owned();
        if target.starts_with("js:") {
            bail!("{name}: replace requires a built-in Operation, not {target:?}");
        }
        let Some(&index) = builtins.get(target) else {
            bail!("{name}: unknown built-in operation replacement target {target:?}");
        };
        if matches!(nodes[index].step.as_ref(), Some(Step::Inference(_))) {
            bail!("{name}: cannot replace inference operation {target:?}; JavaScript Operations do not implement Inference");
        }
        if !replaced.insert(index) {
            bail!("Multiple JavaScript Operations replace {target:?}");
        }
        if names.insert(name.clone(), index).is_some() {
            bail!("Duplicate operation name: {name}");
        }
        // Both the original anchor and the JS name identify the same replacement slot.
        nodes[index].name = name;
        nodes[index].step = Some(Step::Operation(operation.operation));
    }

    let mut placements = Vec::new();
    for operation in insertions {
        let index = nodes.len();
        let name = operation.operation.name().to_owned();
        if names.insert(name.clone(), index).is_some() {
            bail!("Duplicate operation name: {name}");
        }
        placements.push(operation.placement);
        nodes.push(Node {
            name,
            step: Some(Step::Operation(operation.operation)),
            before: Vec::new(),
            after: Vec::new(),
        });
    }

    for (offset, placement) in placements.into_iter().enumerate() {
        let index = roots + offset;
        let target = match &placement {
            OperationPlacement::Before(target) | OperationPlacement::After(target) => target,
            OperationPlacement::Replace(_) => unreachable!("replacements already resolved"),
        };
        let Some(&anchor) = names.get(target) else {
            let mut anchors: Vec<_> = names.keys().map(String::as_str).collect();
            anchors.sort_unstable();
            bail!(
                "{}: unknown operation anchor {target:?}; available anchors: {}",
                nodes[index].name,
                anchors.join(", ")
            );
        };
        match placement {
            OperationPlacement::Before(_) => nodes[anchor].before.push(index),
            OperationPlacement::After(_) => nodes[anchor].after.push(index),
            OperationPlacement::Replace(_) => unreachable!("replacements already resolved"),
        }
    }

    let mut ordered = Vec::with_capacity(nodes.len());
    for root in 0..roots {
        append_group(root, &mut nodes, &mut ordered);
    }
    // Each custom node has one parent. A group disconnected from all roots contains a cycle.
    if ordered.len() != nodes.len() {
        bail!(
            "Cyclic JavaScript operation placement: {}",
            nodes
                .iter()
                .filter(|node| node.step.is_some())
                .map(|node| node.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    Ok(ordered)
}
