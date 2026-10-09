use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;

use super::parser::{Call, FileAnalysis, Symbol};

/// (file_path, symbol_name, definition_line) — line disambiguates same-name
/// functions in the same file (e.g. two `process()` in different impl blocks).
type NodeKey = (PathBuf, String, usize);

const MAX_CALL_GRAPH_DEPTH: u32 = 32;
const MAX_CALL_GRAPH_PATHS: usize = 2_000;

#[derive(Clone)]
pub struct ChainLink {
    pub file: PathBuf,
    pub name: String,
    pub line: usize,
}

struct Node {
    file: PathBuf,
    name: String,
    line: usize,
}

pub struct CallGraph {
    nodes: HashMap<NodeKey, Node>,
    // callee_key → set of caller_keys
    incoming: HashMap<NodeKey, HashSet<NodeKey>>,
    // caller_key → set of callee_keys
    outgoing: HashMap<NodeKey, HashSet<NodeKey>>,
}

impl CallGraph {
    pub fn build(analyses: &[FileAnalysis]) -> Self {
        let mut nodes: HashMap<NodeKey, Node> = HashMap::new();
        let mut incoming: HashMap<NodeKey, HashSet<NodeKey>> = HashMap::new();
        let mut outgoing: HashMap<NodeKey, HashSet<NodeKey>> = HashMap::new();

        for a in analyses {
            let register = |sym: &Symbol, nodes: &mut HashMap<NodeKey, Node>| {
                let key = (a.path.clone(), sym.name.clone(), sym.line);
                nodes.entry(key).or_insert_with(|| Node {
                    file: a.path.clone(),
                    name: sym.name.clone(),
                    line: sym.line,
                });
            };
            for f in &a.functions {
                register(f, &mut nodes);
            }
            for c in &a.classes {
                register(c, &mut nodes);
            }
        }

        // Register <module> pseudo-nodes so top-level calls have a caller key
        for a in analyses {
            let module_key = (a.path.clone(), "<module>".to_string(), 0usize);
            nodes.entry(module_key).or_insert_with(|| Node {
                file: a.path.clone(),
                name: "<module>".to_string(),
                line: 0,
            });
        }

        // Build a name → keys index for resolving cross-file calls
        let mut name_index: HashMap<&str, Vec<NodeKey>> = HashMap::new();
        for key in nodes.keys() {
            name_index.entry(&key.1).or_default().push(key.clone());
        }

        // Build (path, name) → sorted definition lines for caller/callee resolution.
        // When a Call says caller="process" at line 50, we pick the definition
        // of "process" whose line is the largest value ≤ 50 (nearest enclosing).
        let mut def_lines: HashMap<(&PathBuf, &str), Vec<usize>> = HashMap::new();
        for key in nodes.keys() {
            def_lines.entry((&key.0, &key.1)).or_default().push(key.2);
        }
        for lines in def_lines.values_mut() {
            lines.sort_unstable();
        }

        // Build path → language index to prevent cross-language false positives
        let lang_index: HashMap<&PathBuf, &str> =
            analyses.iter().map(|a| (&a.path, a.language)).collect();

        for a in analyses {
            for call in &a.calls {
                // Fall back to <module> pseudo-node for top-level calls
                let caller_key = resolve_caller_key(a, call, &def_lines)
                    .unwrap_or_else(|| (a.path.clone(), "<module>".to_string(), 0));
                // Resolve callee: same-file first, then cross-file (same language only)
                let callee_keys = resolve_callee(a, call, &name_index, &def_lines, &lang_index);
                for callee_key in callee_keys {
                    incoming
                        .entry(callee_key.clone())
                        .or_default()
                        .insert(caller_key.clone());
                    outgoing
                        .entry(caller_key.clone())
                        .or_default()
                        .insert(callee_key);
                }
            }
        }

        Self {
            nodes,
            incoming,
            outgoing,
        }
    }

    pub fn definitions(&self, symbol: &str) -> Vec<ChainLink> {
        self.nodes
            .values()
            .filter(|n| n.name == symbol)
            .map(|n| ChainLink {
                file: n.file.clone(),
                name: n.name.clone(),
                line: n.line,
            })
            .collect()
    }

    pub fn incoming(&self, symbol: &str, depth: u32) -> Vec<Vec<ChainLink>> {
        let starts: Vec<NodeKey> = self
            .nodes
            .keys()
            .filter(|k| k.1 == symbol)
            .cloned()
            .collect();
        self.bfs_chains(&starts, depth, &self.incoming)
    }

    pub fn outgoing(&self, symbol: &str, depth: u32) -> Vec<Vec<ChainLink>> {
        let starts: Vec<NodeKey> = self
            .nodes
            .keys()
            .filter(|k| k.1 == symbol)
            .cloned()
            .collect();
        self.bfs_chains(&starts, depth, &self.outgoing)
    }

    fn bfs_chains(
        &self,
        starts: &[NodeKey],
        depth: u32,
        edges: &HashMap<NodeKey, HashSet<NodeKey>>,
    ) -> Vec<Vec<ChainLink>> {
        if depth == 0 {
            return vec![];
        }
        let depth = depth.min(MAX_CALL_GRAPH_DEPTH);

        let mut chains = Vec::new();
        let mut queue: VecDeque<(Vec<NodeKey>, u32)> = VecDeque::new();

        'starts: for start in starts {
            if let Some(neighbors) = edges.get(start) {
                for neighbor in neighbors {
                    if queue.len() >= MAX_CALL_GRAPH_PATHS {
                        break 'starts;
                    }
                    queue.push_back((vec![start.clone(), neighbor.clone()], 1));
                }
            }
        }

        while let Some((path, d)) = queue.pop_front() {
            let Some(tip) = path.last() else { continue };

            if d >= depth {
                chains.push(self.to_chain_links(&path));
                continue;
            }

            // Cycle detection: don't revisit nodes already in this path
            let visited: HashSet<&NodeKey> = path.iter().collect();

            match edges.get(tip) {
                Some(neighbors) => {
                    let mut extended = false;
                    for neighbor in neighbors {
                        if !visited.contains(neighbor) {
                            if chains.len() + queue.len() >= MAX_CALL_GRAPH_PATHS {
                                break;
                            }
                            let mut new_path = path.clone();
                            new_path.push(neighbor.clone());
                            queue.push_back((new_path, d + 1));
                            extended = true;
                        }
                    }
                    if !extended {
                        chains.push(self.to_chain_links(&path));
                    }
                }
                None => chains.push(self.to_chain_links(&path)),
            }
        }

        chains
    }

    fn to_chain_links(&self, path: &[NodeKey]) -> Vec<ChainLink> {
        path.iter()
            .map(|key| {
                let node = self.nodes.get(key);
                ChainLink {
                    file: key.0.clone(),
                    name: key.1.clone(),
                    line: node.map_or(0, |n| n.line),
                }
            })
            .collect()
    }
}

/// Given a call, find the NodeKey for the caller function. Uses the call's line
/// number to disambiguate when multiple functions share the same name in a file:
/// picks the definition whose line is the largest value ≤ call.line.
fn resolve_caller_key(
    analysis: &FileAnalysis,
    call: &Call,
    def_lines: &HashMap<(&PathBuf, &str), Vec<usize>>,
) -> Option<NodeKey> {
    let caller_name = &call.caller;
    if let Some(lines) = def_lines.get(&(&analysis.path, caller_name.as_str())) {
        let line = match lines.binary_search(&call.line) {
            Ok(idx) => lines[idx],
            Err(0) => return None, // call is before any definition — shouldn't happen
            Err(idx) => lines[idx - 1],
        };
        Some((analysis.path.clone(), caller_name.clone(), line))
    } else {
        None
    }
}

fn resolve_callee(
    analysis: &FileAnalysis,
    call: &Call,
    name_index: &HashMap<&str, Vec<NodeKey>>,
    def_lines: &HashMap<(&PathBuf, &str), Vec<usize>>,
    lang_index: &HashMap<&PathBuf, &str>,
) -> Vec<NodeKey> {
    let callee = &call.callee;
    let caller_lang = analysis.language;

    // Strip scope prefix for qualified calls like Self::method(), Type::new(),
    // HashMap::new(), module::func(). The name index is keyed on bare names
    // (from Symbol.name), but call captures include the full scoped_identifier.
    let bare_name = callee.rsplit("::").next().unwrap_or(callee);

    // Resolve the nearest local definition without scanning and cloning every
    // same-name symbol per call. Equal-distance ties prefer the earlier line.
    if let Some(lines) = def_lines.get(&(&analysis.path, bare_name)) {
        let line = match lines.binary_search_by(|line| {
            #[cfg(test)]
            tests::CALLEE_COMPARISONS.with(|count| count.set(count.get() + 1));
            line.cmp(&call.line)
        }) {
            Ok(idx) => lines[idx],
            Err(0) => lines[0],
            Err(idx) if idx == lines.len() => lines[idx - 1],
            Err(idx) => {
                let before = lines[idx - 1];
                let after = lines[idx];
                if call.line - before <= after - call.line {
                    before
                } else {
                    after
                }
            }
        };
        return vec![(analysis.path.clone(), bare_name.to_string(), line)];
    }

    if let Some(keys) = name_index.get(bare_name) {
        // Cross-file matches filtered to same language only
        keys.iter()
            .filter(|(path, _, _)| lang_index.get(path).copied() == Some(caller_lang))
            .cloned()
            .collect()
    } else {
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use test_case::test_case;

    thread_local! {
        pub(super) static CALLEE_COMPARISONS: Cell<usize> = const { Cell::new(0) };
    }

    fn symbol(name: impl Into<String>, line: usize) -> Symbol {
        Symbol {
            name: name.into(),
            line,
            parent: None,
            detail: None,
        }
    }

    fn analysis(
        path: &str,
        language: &'static str,
        functions: Vec<Symbol>,
        calls: Vec<Call>,
    ) -> FileAnalysis {
        FileAnalysis {
            path: PathBuf::from(path),
            language,
            loc: 1_000,
            functions,
            classes: Vec::new(),
            imports: Vec::new(),
            calls,
        }
    }

    #[test_case(64)]
    #[test_case(256)]
    fn same_file_callee_resolution_has_logarithmic_work_per_call(width: usize) {
        let source: String = (0..width).map(|i| format!(
            "struct Type{i};\nimpl Type{i} {{\n    fn shared() {{}}\n    fn caller_{i}() {{ Self::shared(); }}\n}}\n"
        )).collect();
        let analysis = super::super::parser::Parser::new()
            .analyze_file(std::path::Path::new("repeated.rs"), &source)
            .unwrap();
        assert_eq!(analysis.functions.len(), width * 2);
        assert_eq!(analysis.calls.len(), width);

        CALLEE_COMPARISONS.set(0);
        let graph = CallGraph::build(&[analysis]);
        let comparisons = CALLEE_COMPARISONS.get();
        let budget = width * (width.ilog2() as usize + 2);
        assert!(
            comparisons > 0,
            "callee comparison counter was not exercised"
        );
        assert!(
            comparisons <= budget,
            "{width} calls visited {comparisons} candidates, exceeding the {budget} logarithmic budget"
        );
        for i in 0..width {
            assert_eq!(
                graph.outgoing[&(
                    PathBuf::from("repeated.rs"),
                    format!("caller_{i}"),
                    i * 5 + 4
                )],
                HashSet::from([(
                    PathBuf::from("repeated.rs"),
                    "shared".to_string(),
                    i * 5 + 3
                )])
            );
        }
    }

    #[test_case(5, 10; "before_first")]
    #[test_case(10, 10; "exact")]
    #[test_case(11, 10; "nearest_before")]
    #[test_case(19, 20; "nearest_after")]
    #[test_case(15, 10; "equal_distance_prefers_earlier")]
    #[test_case(45, 40; "after_last")]
    fn same_file_resolution_preserves_nearest_definition(call_line: usize, expected_line: usize) {
        let graph = CallGraph::build(&[
            analysis(
                "local.rs",
                "rust",
                vec![
                    symbol("caller", 1),
                    symbol("shared", 40),
                    symbol("shared", 10),
                    symbol("shared", 20),
                ],
                vec![Call {
                    caller: "caller".to_string(),
                    callee: "Type::shared".to_string(),
                    line: call_line,
                }],
            ),
            analysis(
                "other.rs",
                "rust",
                vec![symbol("shared", call_line)],
                Vec::new(),
            ),
        ]);
        assert_eq!(
            graph.outgoing[&(PathBuf::from("local.rs"), "caller".to_string(), 1)],
            HashSet::from([(
                PathBuf::from("local.rs"),
                "shared".to_string(),
                expected_line
            )])
        );
    }

    #[test]
    fn absent_same_file_definition_preserves_cross_file_language_filter() {
        let graph = CallGraph::build(&[
            analysis(
                "caller.rs",
                "rust",
                vec![symbol("caller", 1)],
                vec![
                    Call {
                        caller: "caller".to_string(),
                        callee: "module::shared".to_string(),
                        line: 2,
                    },
                    Call {
                        caller: "caller".to_string(),
                        callee: "missing".to_string(),
                        line: 3,
                    },
                ],
            ),
            analysis("first.rs", "rust", vec![symbol("shared", 10)], Vec::new()),
            analysis("second.rs", "rust", vec![symbol("shared", 20)], Vec::new()),
            analysis(
                "different.py",
                "python",
                vec![symbol("shared", 30)],
                Vec::new(),
            ),
        ]);
        assert_eq!(
            graph.outgoing[&(PathBuf::from("caller.rs"), "caller".to_string(), 1)],
            HashSet::from([
                (PathBuf::from("first.rs"), "shared".to_string(), 10),
                (PathBuf::from("second.rs"), "shared".to_string(), 20),
            ])
        );
    }

    #[test]
    fn outgoing_paths_are_bounded_for_dense_graphs() {
        let width = 50;
        let mut functions = vec![symbol("root", 1)];
        let mut calls = Vec::new();

        for i in 0..width {
            let name = format!("branch_{i}");
            let line = 100 + i;
            functions.push(symbol(&name, line));
            calls.push(Call {
                caller: "root".to_string(),
                callee: name,
                line: 2 + i,
            });
        }

        for j in 0..width {
            functions.push(symbol(format!("leaf_{j}"), 1_000 + j));
        }

        for i in 0..width {
            for j in 0..width {
                calls.push(Call {
                    caller: format!("branch_{i}"),
                    callee: format!("leaf_{j}"),
                    line: 100 + i,
                });
            }
        }

        let graph = CallGraph::build(&[FileAnalysis {
            path: PathBuf::from("dense.rs"),
            language: "rust",
            loc: 2_000,
            functions,
            classes: Vec::new(),
            imports: Vec::new(),
            calls,
        }]);

        let chains = graph.outgoing("root", 2);

        assert_eq!(chains.len(), MAX_CALL_GRAPH_PATHS);
    }

    #[test]
    fn outgoing_depth_is_bounded() {
        let node_count = MAX_CALL_GRAPH_DEPTH as usize + 20;
        let functions = (0..node_count)
            .map(|i| symbol(format!("node_{i}"), i + 1))
            .collect();
        let calls = (0..node_count - 1)
            .map(|i| Call {
                caller: format!("node_{i}"),
                callee: format!("node_{}", i + 1),
                line: i + 1,
            })
            .collect();
        let graph = CallGraph::build(&[FileAnalysis {
            path: PathBuf::from("deep.rs"),
            language: "rust",
            loc: node_count,
            functions,
            classes: Vec::new(),
            imports: Vec::new(),
            calls,
        }]);

        let chains = graph.outgoing("node_0", u32::MAX);

        assert_eq!(chains.len(), 1);
        assert_eq!(chains[0].len(), MAX_CALL_GRAPH_DEPTH as usize + 1);
    }
}
