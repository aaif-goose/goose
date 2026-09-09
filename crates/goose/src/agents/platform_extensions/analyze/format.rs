use std::collections::{HashMap, HashSet};
use std::fmt::Write;
use std::path::Path;

use super::graph::CallGraph;
use super::parser::FileAnalysis;

const SIZE_LIMIT: usize = 50_000;
const MULTILINE_THRESHOLD: usize = 10;

pub fn format_structure(
    analyses: &[FileAnalysis],
    root: &Path,
    depth: u32,
    total_files: usize,
) -> String {
    let mut out = String::new();

    let total_loc: usize = analyses.iter().map(|a| a.loc).sum();
    let total_funcs: usize = analyses.iter().map(|a| a.functions.len()).sum();
    let total_classes: usize = analyses.iter().map(|a| a.classes.len()).sum();

    let depth_str = if depth == 0 {
        "unlimited".to_string()
    } else {
        format!("depth={}", depth)
    };
    let _ = writeln!(
        out,
        "{} files, {}L, {}F, {}C ({})",
        analyses.len(),
        total_loc,
        total_funcs,
        total_classes,
        depth_str
    );

    let skipped = total_files.saturating_sub(analyses.len());
    if skipped > 0 {
        let _ = writeln!(out, "({} files skipped: no parser)", skipped);
    }

    let mut lang_loc: HashMap<&str, usize> = HashMap::new();
    for a in analyses {
        if !a.language.is_empty() && a.loc > 0 {
            *lang_loc.entry(a.language).or_default() += a.loc;
        }
    }
    if !lang_loc.is_empty() && total_loc > 0 {
        let mut langs: Vec<_> = lang_loc.into_iter().collect();
        langs.sort_by_key(|(_, loc)| std::cmp::Reverse(*loc));
        let parts: Vec<String> = langs
            .iter()
            .map(|(lang, loc)| {
                let pct = (*loc as f64 / total_loc as f64 * 100.0) as u32;
                format!("{} {}%", lang, pct)
            })
            .collect();
        let _ = writeln!(out, "{}", parts.join(" | "));
    }
    out.push('\n');

    let tree = build_dir_tree(analyses, root);
    render_tree(&mut out, &tree, 0);

    out
}

pub fn format_semantic(analysis: &FileAnalysis, root: &Path) -> String {
    let mut out = String::new();

    let display_path = analysis.path.strip_prefix(root).unwrap_or(&analysis.path);

    let _ = write!(
        out,
        "{} [{}L, {}F",
        display_path.display(),
        analysis.loc,
        analysis.functions.len()
    );
    if !analysis.classes.is_empty() {
        let _ = write!(out, ", {}C", analysis.classes.len());
    }
    out.push_str("]\n\n");

    if !analysis.classes.is_empty() {
        let items: Vec<String> = analysis
            .classes
            .iter()
            .map(|c| {
                let detail = c.detail.as_deref().unwrap_or("");
                if detail.is_empty() {
                    format!("{}:{}", c.name, c.line)
                } else {
                    format!("{}:{}{}", c.name, c.line, detail)
                }
            })
            .collect();
        format_symbol_list(&mut out, "C:", &items);
    }

    if !analysis.functions.is_empty() {
        let mut call_counts: HashMap<&str, usize> = HashMap::new();
        for call in &analysis.calls {
            let bare = call.callee.rsplit("::").next().unwrap_or(&call.callee);
            *call_counts.entry(bare).or_default() += 1;
        }

        let items: Vec<String> = analysis
            .functions
            .iter()
            .map(|f| {
                let mut label = String::new();
                if let Some(ref parent) = f.parent {
                    label.push_str(parent);
                    label.push('.');
                }
                label.push_str(&f.name);
                if let Some(ref detail) = f.detail {
                    label.push_str(detail);
                }
                let count = call_counts.get(f.name.as_str()).copied().unwrap_or(0);
                if count > 3 {
                    format!("{}:{}•{}", label, f.line, count)
                } else {
                    format!("{}:{}", label, f.line)
                }
            })
            .collect();
        format_symbol_list(&mut out, "F:", &items);
    }

    if !analysis.imports.is_empty() {
        out.push_str("I: ");
        let items: Vec<String> = analysis
            .imports
            .iter()
            .map(|i| {
                if i.count > 1 {
                    format!("{}({})", i.module, i.count)
                } else {
                    i.module.clone()
                }
            })
            .collect();
        out.push_str(&items.join("; "));
        out.push('\n');
    }

    out
}

fn format_symbol_list(out: &mut String, prefix: &str, items: &[String]) {
    if items.len() > MULTILINE_THRESHOLD {
        let _ = writeln!(out, "{}", prefix);
        for item in items {
            let _ = writeln!(out, "  {}", item);
        }
    } else {
        let _ = write!(out, "{} ", prefix);
        out.push_str(&items.join(" "));
        out.push('\n');
    }
}

pub fn format_focused(
    symbol: &str,
    graph: &CallGraph,
    follow_depth: u32,
    files_analyzed: usize,
    root: &Path,
) -> String {
    let defs = graph.definitions(symbol);

    // Always count direct neighbors at depth=1, independent of follow_depth,
    // so the ref count is accurate even when follow_depth=0.
    let depth1_in = graph.incoming(symbol, 1);
    let depth1_out = graph.outgoing(symbol, 1);

    if defs.is_empty() && depth1_in.is_empty() && depth1_out.is_empty() {
        return format!(
            "Symbol '{}' not found in {} analyzed files.\n",
            symbol, files_analyzed
        );
    }

    let incoming = graph.incoming(symbol, follow_depth);
    let outgoing = graph.outgoing(symbol, follow_depth);

    let mut out = String::new();

    let direct_callers: HashSet<_> = depth1_in
        .iter()
        .filter_map(|chain| chain.get(1))
        .map(|link| (&link.file, &link.name, link.line))
        .collect();

    let direct_callees: HashSet<_> = depth1_out
        .iter()
        .filter_map(|chain| chain.get(1))
        .map(|link| (&link.file, &link.name, link.line))
        .collect();

    let ref_count = direct_callers.len() + direct_callees.len();
    let _ = writeln!(
        out,
        "FOCUS: {} ({} defs, {} refs)\n",
        symbol,
        defs.len(),
        ref_count
    );

    for d in &defs {
        let display = d.file.strip_prefix(root).unwrap_or(&d.file);
        let _ = writeln!(out, "DEF {}:{}:{}", display.display(), d.name, d.line);
    }
    if !defs.is_empty() {
        out.push('\n');
    }

    let (in_prod, in_test) = partition_test_chains(&incoming);
    format_chain_group(&mut out, "IN", &in_prod, root);
    format_chain_group(&mut out, "IN (tests)", &in_test, root);

    let (out_prod, out_test) = partition_test_chains(&outgoing);
    format_chain_group(&mut out, "OUT", &out_prod, root);
    format_chain_group(&mut out, "OUT (tests)", &out_test, root);

    let _ = writeln!(out, "{} files analyzed", files_analyzed);

    out
}

type Chain = Vec<super::graph::ChainLink>;

fn format_chain_link(link: &super::graph::ChainLink, root: &Path) -> String {
    let display = link.file.strip_prefix(root).unwrap_or(&link.file);
    format!("{}:{}:{}", display.display(), link.name, link.line)
}

fn format_chain_group(out: &mut String, label: &str, chains: &[Chain], root: &Path) {
    if chains.is_empty() {
        return;
    }

    let mut formatted: Vec<Vec<String>> = chains
        .iter()
        .map(|chain| {
            chain
                .iter()
                .map(|link| format_chain_link(link, root))
                .collect()
        })
        .collect();
    formatted.sort();

    let _ = writeln!(out, "{}:", label);
    let mut i = 0;
    while i < formatted.len() {
        let chain = &formatted[i];
        let mut group_end = i + 1;
        if chain.len() >= 2 {
            let prefix = &chain[..chain.len() - 1];
            while group_end < formatted.len() {
                let next = &formatted[group_end];
                if next.len() >= 2 && next[..next.len() - 1] == *prefix {
                    group_end += 1;
                } else {
                    break;
                }
            }
        }
        if group_end - i > 1 {
            let prefix = &chain[..chain.len() - 1];
            let _ = writeln!(out, "  {}", prefix.join(" → "));
            for entry in &formatted[i..group_end] {
                if let Some(tail) = entry.last() {
                    let _ = writeln!(out, "    → {}", tail);
                }
            }
        } else {
            let _ = writeln!(out, "  {}", chain.join(" → "));
        }
        i = group_end;
    }
    out.push('\n');
}

fn is_test_chain(chain: &[super::graph::ChainLink]) -> bool {
    chain.iter().any(|link| {
        if link.name.starts_with("test_") || link.name.contains("_test") {
            return true;
        }
        let f = link.file.to_string_lossy();
        // Rust / Python
        f.ends_with("_test.rs")
            || f.ends_with("_test.py")
            // JavaScript / TypeScript
            || f.ends_with(".test.ts")
            || f.ends_with(".test.js")
            || f.ends_with(".test.tsx")
            || f.ends_with(".test.jsx")
            // Go
            || f.ends_with("_test.go")
            // Java
            || f.ends_with("Test.java")
            || f.ends_with("Tests.java")
            // Kotlin
            || f.ends_with("Test.kt")
            // Ruby (RSpec + Minitest)
            || f.ends_with("_spec.rb")
            || f.ends_with("_test.rb")
            // Swift
            || f.ends_with("Test.swift")
            || f.ends_with("Tests.swift")
            // Directory conventions
            || f.contains("/tests/")
            || f.contains("/test/")
            || f.contains("/src/test/")  // Java/Kotlin (Maven/Gradle)
            || f.contains("/spec/")      // Ruby (RSpec)
            || f.contains("/Tests/") // Swift Package Manager
    })
}

fn partition_test_chains(chains: &[Chain]) -> (Vec<Chain>, Vec<Chain>) {
    let mut prod = Vec::new();
    let mut test = Vec::new();
    for chain in chains {
        if is_test_chain(chain) {
            test.push(chain.clone());
        } else {
            prod.push(chain.clone());
        }
    }
    (prod, test)
}

pub fn check_size(output: &str, force: bool) -> Result<String, String> {
    if output.len() > SIZE_LIMIT && !force {
        Err(format!(
            "Output too large ({} chars, limit {}). Use `force: true` to override, or narrow scope with max_depth/focus.",
            output.len(),
            SIZE_LIMIT
        ))
    } else {
        Ok(output.to_string())
    }
}

// --- Tree building internals ---

enum TreeNode {
    Dir {
        name: String,
        children: Vec<TreeNode>,
    },
    File {
        name: String,
        loc: usize,
        funcs: usize,
        classes: usize,
    },
}

fn build_dir_tree(analyses: &[FileAnalysis], root: &Path) -> Vec<TreeNode> {
    let mut entries: Vec<(Vec<String>, &FileAnalysis)> = analyses
        .iter()
        .filter_map(|a| {
            let rel = a.path.strip_prefix(root).ok()?;
            let parts: Vec<String> = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().to_string())
                .collect();
            Some((parts, a))
        })
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    build_subtree(&entries, 0)
}

fn build_subtree(entries: &[(Vec<String>, &FileAnalysis)], depth: usize) -> Vec<TreeNode> {
    let mut nodes: Vec<TreeNode> = Vec::new();
    let mut i = 0;

    while i < entries.len() {
        let (parts, analysis) = &entries[i];
        if depth >= parts.len() {
            i += 1;
            continue;
        }

        let name = &parts[depth];

        if depth + 1 == parts.len() {
            nodes.push(TreeNode::File {
                name: name.clone(),
                loc: analysis.loc,
                funcs: analysis.functions.len(),
                classes: analysis.classes.len(),
            });
            i += 1;
        } else {
            let mut j = i + 1;
            while j < entries.len() && entries[j].0.len() > depth && entries[j].0[depth] == *name {
                j += 1;
            }
            let children = build_subtree(&entries[i..j], depth + 1);
            nodes.push(TreeNode::Dir {
                name: name.clone(),
                children,
            });
            i = j;
        }
    }

    nodes.sort_by(|a, b| {
        let a_is_dir = matches!(a, TreeNode::Dir { .. });
        let b_is_dir = matches!(b, TreeNode::Dir { .. });
        match (a_is_dir, b_is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => node_name(a).cmp(node_name(b)),
        }
    });

    nodes
}

fn node_name(node: &TreeNode) -> &str {
    match node {
        TreeNode::Dir { name, .. } | TreeNode::File { name, .. } => name,
    }
}

fn render_tree(out: &mut String, nodes: &[TreeNode], indent: usize) {
    let prefix = "  ".repeat(indent);
    for node in nodes {
        match node {
            TreeNode::Dir { name, children } => {
                let _ = writeln!(out, "{}{}/", prefix, name);
                render_tree(out, children, indent + 1);
            }
            TreeNode::File {
                name,
                loc,
                funcs,
                classes,
            } => {
                let _ = write!(out, "{}{} [{}L, {}F", prefix, name, loc, funcs);
                if *classes > 0 {
                    let _ = write!(out, ", {}C", classes);
                }
                out.push_str("]\n");
            }
        }
    }
}
