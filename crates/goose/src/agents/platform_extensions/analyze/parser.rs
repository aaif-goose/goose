use std::path::{Path, PathBuf};
use tree_sitter::{Language, Parser as TsParser, Query, QueryCursor, StreamingIterator};

use super::languages::{lang_for_ext, LangInfo};

// ── Types ──────────────────────────────────────────────────────────────

pub struct FileAnalysis {
    pub path: PathBuf,
    pub language: &'static str,
    pub loc: usize,
    pub functions: Vec<Symbol>,
    pub classes: Vec<Symbol>,
    pub imports: Vec<Import>,
    pub calls: Vec<Call>,
}

pub struct Symbol {
    pub name: String,
    pub line: usize,
    pub parent: Option<String>,
    pub detail: Option<String>,
}

pub struct Import {
    pub module: String,
    pub count: usize,
}

pub struct Call {
    pub caller: String,
    pub callee: String,
    pub line: usize,
}

// ── Parser ─────────────────────────────────────────────────────────────

#[derive(Default)]
pub struct Parser;

impl Parser {
    pub fn new() -> Self {
        Self
    }

    pub fn analyze_file(&self, path: &Path, source: &str) -> Option<FileAnalysis> {
        let ext = path.extension()?.to_str()?;
        let info = lang_for_ext(ext)?;
        let lang = (info.language)();

        let mut parser = TsParser::new();
        parser.set_language(&lang).ok()?;
        let tree = parser.parse(source, None)?;
        let root = tree.root_node();

        let loc = source.lines().count();
        let functions = extract_functions(&lang, info, root, source);
        let classes = extract_classes(&lang, info, root, source);
        let imports = extract_imports(&lang, info.queries.imports, root, source);
        let calls = extract_calls(&lang, info.queries.calls, root, source, info);

        Some(FileAnalysis {
            path: path.to_path_buf(),
            language: info.name,
            loc,
            functions,
            classes,
            imports,
            calls,
        })
    }
}

// ── Query Runners ──────────────────────────────────────────────────────

fn extract_functions(
    lang: &Language,
    info: &LangInfo,
    root: tree_sitter::Node,
    source: &str,
) -> Vec<Symbol> {
    let Ok(query) = Query::new(lang, info.queries.functions) else {
        return vec![];
    };
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, root, source.as_bytes());
    let mut symbols = Vec::new();

    while let Some(m) = matches.next() {
        for cap in m.captures {
            if query.capture_names()[cap.index as usize] == "name" {
                // `Foo x(a, b);` inside a body parses as a function declarator
                if is_c_family(info.name) && has_ancestor_of_kind(cap.node, "compound_statement") {
                    continue;
                }
                let name_node = leaf_name_node(cap.node);
                let name = node_text(source, &name_node).to_string();
                let line = name_node.start_position().row + 1;
                let parent = find_enclosing_class(cap.node, source, info)
                    .or_else(|| qualified_scope(cap.node, source));
                let detail = extract_fn_signature(cap.node, source);
                symbols.push(Symbol {
                    name,
                    line,
                    parent,
                    detail,
                });
            }
        }
    }

    // Swift init/deinit declarations don't have a name child, so the query
    // can't capture them. Walk the tree to find them and add as symbols.
    if info.name == "swift" {
        collect_init_deinit(root, source, info, &mut symbols);
    }

    symbols
}

/// Recursively collect Swift init_declaration and deinit_declaration nodes.
fn collect_init_deinit(
    node: tree_sitter::Node,
    source: &str,
    info: &LangInfo,
    symbols: &mut Vec<Symbol>,
) {
    for i in 0..node.child_count() as u32 {
        if let Some(child) = node.child(i) {
            match child.kind() {
                "init_declaration" => {
                    symbols.push(Symbol {
                        name: "init".to_string(),
                        line: child.start_position().row + 1,
                        parent: find_enclosing_class(child, source, info),
                        detail: extract_fn_signature_from_node(child, source),
                    });
                }
                "deinit_declaration" => {
                    symbols.push(Symbol {
                        name: "deinit".to_string(),
                        line: child.start_position().row + 1,
                        parent: find_enclosing_class(child, source, info),
                        detail: extract_fn_signature_from_node(child, source),
                    });
                }
                _ => {}
            }
            // Recurse into class/protocol bodies to find nested init/deinit
            collect_init_deinit(child, source, info, symbols);
        }
    }
}

fn extract_classes(
    lang: &Language,
    info: &LangInfo,
    root: tree_sitter::Node,
    source: &str,
) -> Vec<Symbol> {
    let Ok(query) = Query::new(lang, info.queries.classes) else {
        return vec![];
    };
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, root, source.as_bytes());
    let mut symbols = Vec::new();

    while let Some(m) = matches.next() {
        for cap in m.captures {
            if query.capture_names()[cap.index as usize] == "name" {
                let name_text = node_text(source, &cap.node).to_string();
                let line = cap.node.start_position().row + 1;

                let inheritance = cap
                    .node
                    .parent()
                    .map(|p| extract_inheritance(info.name, &p, source))
                    .filter(|s| !s.is_empty());

                let fields = extract_class_detail(cap.node, source, info);
                let detail = match (&inheritance, &fields) {
                    (Some(inh), Some(f)) => Some(format!("({}) {}", inh, f)),
                    (Some(inh), None) => Some(format!("({})", inh)),
                    (None, Some(f)) => Some(f.clone()),
                    (None, None) => None,
                };

                symbols.push(Symbol {
                    name: name_text,
                    line,
                    parent: None,
                    detail,
                });
            }
        }
    }
    symbols
}

/// Extract the superclass / extends / implements target from a class declaration node.
/// Returns an empty string if no inheritance is detected.
/// The `class_node` is the parent of the name node (i.e. the full class declaration).
fn extract_inheritance(lang_name: &str, class_node: &tree_sitter::Node, source: &str) -> String {
    match lang_name {
        // Python: class Foo(Bar, Baz) → argument_list sibling
        "python" => {
            if let Some(supers) = find_child_by_kind(class_node, "argument_list") {
                let super_text = node_text(source, &supers).trim();
                let inner = super_text
                    .strip_prefix('(')
                    .and_then(|s| s.strip_suffix(')'))
                    .unwrap_or("");
                if !inner.is_empty() {
                    return inner.to_string();
                }
            }
            String::new()
        }

        // TypeScript / TSX: class Foo extends Bar implements Baz { ... }
        // class_declaration → class_heritage → extends_clause → type_identifier
        // interface_declaration → extends_type_clause → type_identifier
        "typescript" | "tsx" => {
            if let Some(heritage) = find_child_by_kind(class_node, "class_heritage") {
                if let Some(extends_clause) = find_child_by_kind(&heritage, "extends_clause") {
                    if let Some(ti) = find_descendant_by_kind(&extends_clause, "type_identifier")
                        .or_else(|| find_descendant_by_kind(&extends_clause, "identifier"))
                    {
                        return node_text(source, &ti).to_string();
                    }
                }
            }
            if let Some(extends_clause) = find_child_by_kind(class_node, "extends_type_clause") {
                if let Some(ti) = find_descendant_by_kind(&extends_clause, "type_identifier")
                    .or_else(|| find_descendant_by_kind(&extends_clause, "identifier"))
                {
                    return node_text(source, &ti).to_string();
                }
            }
            String::new()
        }

        // JavaScript: class Foo extends Bar { ... }
        // class_declaration → class_heritage → identifier | member_expression | call_expression
        "javascript" => {
            if let Some(heritage) = find_child_by_kind(class_node, "class_heritage") {
                // Get the full extends expression (could be identifier, member_expression, etc.)
                for i in 0..heritage.child_count() as u32 {
                    if let Some(child) = heritage.child(i) {
                        let text = node_text(source, &child).trim();
                        if !text.is_empty() && text != "extends" {
                            return text.to_string();
                        }
                    }
                }
            }
            String::new()
        }

        // Java: class Foo extends Bar implements Baz { ... }
        // class_declaration → superclass → type_identifier
        "java" => {
            if let Some(superclass) = find_child_by_kind(class_node, "superclass") {
                if let Some(ti) = find_descendant_by_kind(&superclass, "type_identifier")
                    .or_else(|| find_descendant_by_kind(&superclass, "identifier"))
                {
                    return node_text(source, &ti).to_string();
                }
            }
            if let Some(extends) = find_child_by_kind(class_node, "extends_interfaces") {
                if let Some(ti) = find_descendant_by_kind(&extends, "type_identifier")
                    .or_else(|| find_descendant_by_kind(&extends, "identifier"))
                {
                    return node_text(source, &ti).to_string();
                }
            }
            String::new()
        }

        // Kotlin: class Foo : Bar(), Baz { ... }
        // class_declaration → delegation_specifiers → delegation_specifier → user_type → type_identifier
        "kotlin" => {
            if let Some(specs) = find_child_by_kind(class_node, "delegation_specifiers") {
                if let Some(spec) = find_child_by_kind(&specs, "delegation_specifier") {
                    // Try user_type → type_identifier first
                    if let Some(ut) = find_child_by_kind(&spec, "user_type") {
                        if let Some(ti) = find_descendant_by_kind(&ut, "type_identifier")
                            .or_else(|| find_descendant_by_kind(&ut, "identifier"))
                        {
                            return node_text(source, &ti).to_string();
                        }
                    }
                    // Fallback: constructor_invocation → user_type
                    if let Some(ci) = find_child_by_kind(&spec, "constructor_invocation") {
                        if let Some(ut) = find_child_by_kind(&ci, "user_type") {
                            if let Some(ti) = find_descendant_by_kind(&ut, "type_identifier")
                                .or_else(|| find_descendant_by_kind(&ut, "identifier"))
                            {
                                return node_text(source, &ti).to_string();
                            }
                        }
                    }
                }
            }
            String::new()
        }

        // Ruby: class Foo < Bar
        // class → superclass → constant | scope_resolution
        "ruby" => {
            if let Some(superclass) = find_child_by_kind(class_node, "superclass") {
                if let Some(c) = find_child_by_kind(&superclass, "scope_resolution") {
                    return node_text(source, &c).to_string();
                }
                if let Some(c) = find_child_by_kind(&superclass, "constant") {
                    return node_text(source, &c).to_string();
                }
            }
            String::new()
        }

        // Swift: class Foo: Bar, Protocol { ... }
        // class_declaration → inheritance_specifier → type_identifier
        "swift" => {
            if let Some(inh) = find_child_by_kind(class_node, "inheritance_specifier") {
                if let Some(ti) = find_descendant_by_kind(&inh, "user_type") {
                    if let Some(id) = find_descendant_by_kind(&ti, "type_identifier") {
                        return node_text(source, &id).to_string();
                    }
                }
                if let Some(ti) = find_descendant_by_kind(&inh, "type_identifier") {
                    return node_text(source, &ti).to_string();
                }
            }
            String::new()
        }

        // C++: class Foo : public Bar, private Baz<T> { ... }
        // class_specifier → base_class_clause → type_identifier | template_type | qualified_identifier
        "cpp" => find_child_by_kind(class_node, "base_class_clause")
            .map(|clause| {
                (0..clause.named_child_count() as u32)
                    .filter_map(|i| clause.named_child(i))
                    .filter(|base| base.kind() != "access_specifier")
                    .map(|base| node_text(source, &base))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default(),

        // Rust: impl Display for MyType → "MyType(impl Display)"
        // impl_item with "for" keyword: trait is the first type_identifier, type is after "for"
        "rust" => {
            if class_node.kind() != "impl_item" {
                return String::new();
            }
            let mut has_for = false;
            for i in 0..class_node.child_count() as u32 {
                if let Some(child) = class_node.child(i) {
                    if node_text(source, &child) == "for" {
                        has_for = true;
                        break;
                    }
                }
            }
            if !has_for {
                // Inherent impl (no trait) — return "impl" to distinguish from struct definition
                return "impl".to_string();
            }
            let mut trait_name = String::new();
            let mut found_for = false;
            for i in 0..class_node.child_count() as u32 {
                if let Some(child) = class_node.child(i) {
                    if node_text(source, &child) == "for" {
                        found_for = true;
                    } else if !found_for
                        && (child.kind() == "type_identifier"
                            || child.kind() == "scoped_type_identifier"
                            || child.kind() == "generic_type")
                    {
                        trait_name = node_text(source, &child).to_string();
                    }
                }
            }
            if !trait_name.is_empty() {
                return format!("impl {}", trait_name);
            }
            String::new()
        }

        _ => String::new(),
    }
}

/// Walk up from a function node to find the nearest enclosing class-like container.
fn find_enclosing_class(node: tree_sitter::Node, source: &str, info: &LangInfo) -> Option<String> {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if info.class_kinds.contains(&parent.kind()) {
            if parent.kind() == "impl_item" {
                // For trait impls (impl Trait for Type), get the type after "for"
                let mut found_for = false;
                for i in 0..parent.child_count() as u32 {
                    if let Some(child) = parent.child(i) {
                        if node_text(source, &child) == "for" {
                            found_for = true;
                        } else if found_for
                            && (child.kind() == "type_identifier"
                                || child.kind() == "generic_type"
                                || child.kind() == "scoped_type_identifier")
                        {
                            return Some(node_text(source, &child).to_string());
                        }
                    }
                }
                // Inherent impl — first type_identifier is the type itself
                return find_child_by_kind(&parent, "type_identifier")
                    .map(|n| node_text(source, &n).to_string());
            }
            // Go method_declaration: func (r *ReceiverType) Method() — extract receiver type
            if parent.kind() == "method_declaration" {
                if let Some(params) = find_child_by_kind(&parent, "parameter_list") {
                    if let Some(ti) = find_descendant_by_kind(&params, "type_identifier") {
                        return Some(node_text(source, &ti).to_string());
                    }
                }
                return None;
            }
            // For Go type_declaration, look inside type_spec
            if parent.kind() == "type_declaration" {
                for i in 0..parent.child_count() as u32 {
                    if let Some(child) = parent.child(i) {
                        if child.kind() == "type_spec" {
                            return find_child_by_kind(&child, "type_identifier")
                                .map(|n| node_text(source, &n).to_string());
                        }
                    }
                }
                return None;
            }
            let name_kinds = &[
                "identifier",
                "type_identifier",
                "constant",
                "simple_identifier",
            ];
            for kind in name_kinds {
                if let Some(n) = find_child_by_kind(&parent, kind) {
                    return Some(node_text(source, &n).to_string());
                }
            }
            return None;
        }
        cur = parent;
    }
    None
}

/// Extract a compact function signature: "(params) -> ReturnType"
fn extract_fn_signature(name_node: tree_sitter::Node, source: &str) -> Option<String> {
    let fn_node = name_node.parent()?;
    if fn_node.kind() == "function_declarator" {
        return extract_c_family_signature(fn_node, source);
    }
    extract_fn_signature_from_node(fn_node, source)
}

/// The return type lives on the enclosing definition/declaration, not on the declarator.
fn extract_c_family_signature(declarator: tree_sitter::Node, source: &str) -> Option<String> {
    let params =
        extract_fn_signature_from_node(declarator, source).unwrap_or_else(|| "()".to_string());

    let mut indirection = String::new();
    let mut owner = None;
    for ancestor in std::iter::successors(declarator.parent(), |n| n.parent()) {
        match ancestor.kind() {
            "pointer_declarator" => indirection.push('*'),
            "reference_declarator" => indirection.push('&'),
            "function_definition" | "declaration" | "field_declaration" => {
                owner = Some(ancestor);
                break;
            }
            _ => {}
        }
    }

    let return_type = owner
        .and_then(|n| n.child_by_field_name("type"))
        .map(|t| format!("{}{}", node_text(source, &t).trim(), indirection))
        .filter(|t| t != "void");

    match return_type {
        Some(ret) => Some(format!("{}->{}", params, truncate(&ret, 30))),
        None if params == "()" => None,
        None => Some(params),
    }
}

/// Extract a compact function signature directly from the function node.
/// Used for nodes like Swift init/deinit that don't have a name child.
fn extract_fn_signature_from_node(fn_node: tree_sitter::Node, source: &str) -> Option<String> {
    let mut parts = String::new();

    let param_kinds = &[
        "parameters",
        "formal_parameters",
        "parameter_list",
        "function_value_parameters",
        "method_parameters",
        "lambda_parameters",
    ];
    let params_node = param_kinds
        .iter()
        .find_map(|kind| find_child_by_kind(&fn_node, kind));

    if let Some(pn) = params_node {
        let raw = node_text(source, &pn);
        if raw.len() <= 60 {
            parts.push_str(raw);
        } else {
            let count = raw.matches(',').count() + 1;
            parts.push_str(&format!("({} args)", count));
        }
    } else {
        parts.push_str("()");
    }

    let ret_kinds = &["type", "return_type", "type_annotation"];
    // For Rust: look for a child that is "->" followed by a type
    // For Python: look for "return_type" or "type" child
    // Generic approach: scan children for return type indicators
    for i in 0..fn_node.child_count() as u32 {
        if let Some(child) = fn_node.child(i) {
            if ret_kinds.contains(&child.kind()) {
                let ret_text = node_text(source, &child).trim().to_string();
                if !ret_text.is_empty() {
                    let ret_text = ret_text
                        .trim_start_matches("->")
                        .trim_start_matches(':')
                        .trim();
                    if !ret_text.is_empty() {
                        parts.push_str("->");
                        parts.push_str(&truncate(ret_text, 30));
                    }
                }
                break;
            }
            // Rust uses "->" as a literal anonymous child, then a type child follows
            if node_text(source, &child) == "->" {
                if let Some(type_child) = fn_node.child(i + 1) {
                    let ret_text = node_text(source, &type_child).trim();
                    if !ret_text.is_empty() {
                        parts.push_str("->");
                        parts.push_str(&truncate(ret_text, 30));
                    }
                }
                break;
            }
        }
    }

    if parts == "()" {
        return None;
    }
    Some(parts)
}

/// Extract a compact detail for a class/struct: field names or count.
fn extract_class_detail(
    name_node: tree_sitter::Node,
    source: &str,
    info: &LangInfo,
) -> Option<String> {
    let class_node = name_node.parent()?;

    let (body_kinds, field_kinds): (&[&str], &[&str]) = match info.name {
        "rust" => (&["field_declaration_list"], &["field_declaration"]),
        "go" => (
            &["field_declaration_list", "struct_type"],
            &["field_declaration"],
        ),
        "java" | "kotlin" => (&["class_body"], &["field_declaration"]),
        "c" | "cpp" => (&["field_declaration_list"], &["field_declaration"]),
        _ => return None, // Skip Python (hard), JS/TS/Ruby/Swift for now
    };

    let body = body_kinds
        .iter()
        .find_map(|kind| find_descendant_by_kind(&class_node, kind))?;

    let mut fields: Vec<String> = Vec::new();
    collect_field_names(&body, field_kinds, source, &mut fields);

    if fields.is_empty() {
        return None;
    }

    if fields.len() <= 5 {
        Some(format!("{{{}}}", fields.join(",")))
    } else {
        Some(format!("{{{}f}}", fields.len()))
    }
}

fn collect_field_names(
    node: &tree_sitter::Node,
    field_kinds: &[&str],
    source: &str,
    out: &mut Vec<String>,
) {
    for i in 0..node.child_count() as u32 {
        if let Some(child) = node.child(i) {
            if field_kinds.contains(&child.kind()) {
                // Java/Kotlin: field name is inside variable_declarator, not a direct child.
                // e.g. (field_declaration type: (type_identifier) declarator: (variable_declarator name: (identifier)))
                if let Some(vd) = find_child_by_kind(&child, "variable_declarator") {
                    if let Some(n) = find_child_by_kind(&vd, "identifier") {
                        out.push(node_text(source, &n).to_string());
                        continue;
                    }
                }
                // C/C++: the name sits under pointer/array declarators. Method
                // declarations are skipped; function pointers `(*cb)(int)` are fields.
                if let Some(declarator) = child.child_by_field_name("declarator") {
                    if let Some(n) = find_descendant_by_kind(&declarator, "field_identifier") {
                        let is_method = find_descendant_by_kind(&declarator, "function_declarator")
                            .and_then(|f| f.child_by_field_name("declarator"))
                            .is_some_and(|d| d.kind() != "parenthesized_declarator");
                        if !is_method {
                            out.push(node_text(source, &n).to_string());
                        }
                        continue;
                    }
                }
                // Default: direct child lookup (Rust, Go, etc.)
                let name_kinds = &["field_identifier", "identifier", "type_identifier"];
                for nk in name_kinds {
                    if let Some(n) = find_child_by_kind(&child, nk) {
                        out.push(node_text(source, &n).to_string());
                        break;
                    }
                }
            }
        }
    }
}

fn find_child_by_kind<'a>(
    node: &tree_sitter::Node<'a>,
    kind: &str,
) -> Option<tree_sitter::Node<'a>> {
    (0..node.child_count() as u32)
        .filter_map(|i| node.child(i))
        .find(|c| c.kind() == kind)
}

fn find_descendant_by_kind<'a>(
    node: &tree_sitter::Node<'a>,
    kind: &str,
) -> Option<tree_sitter::Node<'a>> {
    if node.kind() == kind {
        return Some(*node);
    }
    for i in 0..node.child_count() as u32 {
        if let Some(child) = node.child(i) {
            if let Some(found) = find_descendant_by_kind(&child, kind) {
                return Some(found);
            }
        }
    }
    None
}

fn extract_imports(
    lang: &Language,
    query_src: &str,
    root: tree_sitter::Node,
    source: &str,
) -> Vec<Import> {
    let Ok(query) = Query::new(lang, query_src) else {
        return vec![];
    };
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, root, source.as_bytes());
    let mut imports: Vec<Import> = Vec::new();

    while let Some(m) = matches.next() {
        for cap in m.captures {
            if query.capture_names()[cap.index as usize] != "path" {
                continue;
            }
            let raw = node_text(source, &cap.node);
            let module = normalize_import(raw.trim());
            if let Some(existing) = imports.iter_mut().find(|i| i.module == module) {
                existing.count += 1;
            } else {
                imports.push(Import { module, count: 1 });
            }
        }
    }
    imports
}

fn normalize_import(s: &str) -> String {
    let s = s
        .trim_start_matches("use ")
        .trim_start_matches("import ")
        .trim_start_matches("from ")
        .trim_start_matches("require_relative ")
        .trim_start_matches("require ")
        .trim_start_matches("load ")
        .trim_end_matches(';')
        .trim()
        .trim_matches(|c| c == '\'' || c == '"');
    // Handle Python "from X import Y" → keep just "X"
    let s = s.split(" import ").next().unwrap_or(s);
    // JS/TS: "React from 'react'" or "{ useState } from 'react'" → "react"
    // After stripping "import " prefix, we have "React from 'react'"
    // Extract the module path after " from " and strip quotes
    if let Some(idx) = s.find(" from ") {
        let module = s
            .get(idx + 6..)
            .unwrap_or("")
            .trim()
            .trim_matches(|c: char| c == '\'' || c == '"');
        if !module.is_empty() {
            return module.to_string();
        }
    }
    // Rust: strip brace groups like "std::collections::{HashMap, HashSet}"
    match s.find("::{") {
        Some(i) => s.get(..i).unwrap_or(s).to_string(),
        None => s.to_string(),
    }
}

fn extract_calls(
    lang: &Language,
    query_src: &str,
    root: tree_sitter::Node,
    source: &str,
    info: &LangInfo,
) -> Vec<Call> {
    let Ok(query) = Query::new(lang, query_src) else {
        return vec![];
    };
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, root, source.as_bytes());
    let mut calls = Vec::new();

    while let Some(m) = matches.next() {
        for cap in m.captures {
            if query.capture_names()[cap.index as usize] != "name" {
                continue;
            }
            let callee_node = leaf_name_node(cap.node);
            let callee = node_text(source, &callee_node).to_string();
            let line = callee_node.start_position().row + 1;
            let caller =
                find_enclosing_fn(cap.node, source, info).unwrap_or_else(|| "<module>".to_string());
            calls.push(Call {
                caller,
                callee,
                line,
            });
        }
    }
    calls
}

fn find_enclosing_fn(node: tree_sitter::Node, source: &str, info: &LangInfo) -> Option<String> {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if info.fn_kinds.contains(&parent.kind()) {
            if is_c_family(info.name) {
                if let Some(name) = c_family_fn_name(parent, source) {
                    return Some(name);
                }
            }
            // Special case: Swift init/deinit
            if parent.kind() == "init_declaration" {
                return Some("init".into());
            }
            if parent.kind() == "deinit_declaration" {
                return Some("deinit".into());
            }
            // variable_declarator: only treat as function scope if value is arrow/function
            if parent.kind() == "variable_declarator" {
                let is_fn_value = find_child_by_kind(&parent, "arrow_function").is_some()
                    || find_child_by_kind(&parent, "function").is_some();
                if !is_fn_value {
                    cur = parent;
                    continue; // skip — not a function declarator
                }
            }
            // If this is an anonymous function-like node (closure, async block, arrow
            // function with no name), keep walking up to find the enclosing named function.
            if let Some(name) = find_child_text(&parent, info.fn_name_kinds, source) {
                return Some(name);
            }
        }
        cur = parent;
    }
    None
}

fn is_c_family(lang_name: &str) -> bool {
    matches!(lang_name, "c" | "cpp")
}

fn has_ancestor_of_kind(node: tree_sitter::Node, kind: &str) -> bool {
    std::iter::successors(node.parent(), |n| n.parent()).any(|n| n.kind() == kind)
}

/// For `a::b::Foo::out`, the innermost `qualified_identifier` is the one holding `Foo` and `out`.
fn innermost_qualified(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    if node.kind() != "qualified_identifier" {
        return None;
    }
    let name = node.child_by_field_name("name")?;
    innermost_qualified(name).or(Some(node))
}

/// Calls and definitions are matched by bare name, so `Foo::out` resolves to `out`.
fn leaf_name_node(node: tree_sitter::Node) -> tree_sitter::Node {
    innermost_qualified(node)
        .and_then(|q| q.child_by_field_name("name"))
        .unwrap_or(node)
}

fn qualified_scope(node: tree_sitter::Node, source: &str) -> Option<String> {
    let scope = innermost_qualified(node)?.child_by_field_name("scope")?;
    Some(node_text(source, &scope).to_string())
}

fn c_family_fn_name(fn_node: tree_sitter::Node, source: &str) -> Option<String> {
    let mut node = fn_node.child_by_field_name("declarator")?;
    while matches!(
        node.kind(),
        "pointer_declarator"
            | "reference_declarator"
            | "function_declarator"
            | "parenthesized_declarator"
            | "attributed_declarator"
    ) {
        node = node
            .child_by_field_name("declarator")
            .or_else(|| node.named_child(0))?;
    }
    Some(node_text(source, &leaf_name_node(node)).to_string())
}

fn find_child_text(node: &tree_sitter::Node, kinds: &[&str], source: &str) -> Option<String> {
    (0..node.child_count() as u32)
        .filter_map(|i| node.child(i))
        .find(|c| kinds.contains(&c.kind()))
        .map(|c| node_text(source, &c).to_string())
}

/// Truncate a string to at most `max` chars, appending "..." if truncated.
fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let limit = max.saturating_sub(3);
    // Walk back to a valid char boundary
    let end = (0..=limit)
        .rev()
        .find(|&i| s.is_char_boundary(i))
        .unwrap_or(0);
    let prefix = s.get(..end).unwrap_or("");
    format!("{}...", prefix)
}

fn node_text<'a>(source: &'a str, node: &tree_sitter::Node) -> &'a str {
    source.get(node.byte_range()).unwrap_or("")
}
