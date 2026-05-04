use tree_sitter::Language;

// ── Types ──────────────────────────────────────────────────────────────

pub struct LangInfo {
    pub name: &'static str,
    pub extensions: &'static [&'static str],
    pub language: fn() -> Language,
    pub queries: LangQueries,
    pub fn_kinds: &'static [&'static str],
    pub fn_name_kinds: &'static [&'static str],
    pub class_kinds: &'static [&'static str],
}

pub struct LangQueries {
    pub functions: &'static str,
    pub classes: &'static str,
    pub imports: &'static str,
    pub calls: &'static str,
}

// ── Language Registry ──────────────────────────────────────────────────

static LANGUAGES: &[LangInfo] = &[
    LangInfo {
        name: "rust",
        extensions: &["rs"],
        language: || tree_sitter_rust::LANGUAGE.into(),
        fn_kinds: &["function_item"],
        fn_name_kinds: &["identifier"],
        class_kinds: &["impl_item", "struct_item", "trait_item", "enum_item"],
        queries: LangQueries {
            functions: r#"
                (function_item name: (identifier) @name)
            "#,
            classes: r#"
                (impl_item type: (type_identifier) @name)
                (struct_item name: (type_identifier) @name)
                (trait_item name: (type_identifier) @name)
                (enum_item name: (type_identifier) @name)
            "#,
            imports: r#"
                (use_declaration) @path
            "#,
            calls: r#"
                (call_expression function: (identifier) @name)
                (call_expression function: (field_expression field: (field_identifier) @name))
                (call_expression function: (scoped_identifier) @name)
                (macro_invocation macro: (identifier) @name)
            "#,
        },
    },
    LangInfo {
        name: "python",
        extensions: &["py", "pyi"],
        language: || tree_sitter_python::LANGUAGE.into(),
        fn_kinds: &["function_definition"],
        fn_name_kinds: &["identifier"],
        class_kinds: &["class_definition"],
        queries: LangQueries {
            functions: r#"
                (function_definition name: (identifier) @name)
            "#,
            classes: r#"
                (class_definition name: (identifier) @name)
            "#,
            imports: r#"
                (import_statement) @path
                (import_from_statement) @path
            "#,
            calls: r#"
                (call function: (identifier) @name)
                (call function: (attribute attribute: (identifier) @name))
                (decorator (identifier) @name)
                (decorator (attribute attribute: (identifier) @name))
            "#,
        },
    },
    LangInfo {
        name: "javascript",
        extensions: &["js", "jsx", "mjs", "cjs"],
        language: || tree_sitter_javascript::LANGUAGE.into(),
        fn_kinds: &[
            "function_declaration",
            "generator_function_declaration",
            "method_definition",
            "variable_declarator",
        ],
        fn_name_kinds: &["identifier", "property_identifier"],
        class_kinds: &["class_declaration"],
        queries: LangQueries {
            functions: r#"
                (function_declaration name: (identifier) @name)
                (generator_function_declaration name: (identifier) @name)
                (method_definition name: (property_identifier) @name)
                (lexical_declaration
                  (variable_declarator
                    name: (identifier) @name
                    value: (arrow_function)))
            "#,
            classes: r#"
                (class_declaration name: (identifier) @name)
            "#,
            imports: r#"
                (import_statement) @path
            "#,
            calls: r#"
                (call_expression function: (identifier) @name)
                (call_expression function: (member_expression property: (property_identifier) @name))
                (new_expression constructor: (identifier) @name)
            "#,
        },
    },
    LangInfo {
        name: "typescript",
        extensions: &["ts"],
        language: || tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        fn_kinds: &[
            "function_declaration",
            "generator_function_declaration",
            "method_definition",
            "variable_declarator",
        ],
        fn_name_kinds: &["identifier", "property_identifier"],
        class_kinds: &["class_declaration", "interface_declaration"],
        queries: LangQueries {
            functions: r#"
                (function_declaration name: (identifier) @name)
                (generator_function_declaration name: (identifier) @name)
                (method_definition name: (property_identifier) @name)
                (lexical_declaration
                  (variable_declarator
                    name: (identifier) @name
                    value: (arrow_function)))
            "#,
            classes: r#"
                (class_declaration name: (type_identifier) @name)
                (interface_declaration name: (type_identifier) @name)
            "#,
            imports: r#"
                (import_statement) @path
            "#,
            calls: r#"
                (call_expression function: (identifier) @name)
                (call_expression function: (member_expression property: (property_identifier) @name))
                (new_expression constructor: (identifier) @name)
            "#,
        },
    },
    LangInfo {
        name: "tsx",
        extensions: &["tsx"],
        language: || tree_sitter_typescript::LANGUAGE_TSX.into(),
        fn_kinds: &[
            "function_declaration",
            "generator_function_declaration",
            "method_definition",
            "variable_declarator",
        ],
        fn_name_kinds: &["identifier", "property_identifier"],
        class_kinds: &["class_declaration", "interface_declaration"],
        queries: LangQueries {
            functions: r#"
                (function_declaration name: (identifier) @name)
                (generator_function_declaration name: (identifier) @name)
                (method_definition name: (property_identifier) @name)
                (lexical_declaration
                  (variable_declarator
                    name: (identifier) @name
                    value: (arrow_function)))
            "#,
            classes: r#"
                (class_declaration name: (type_identifier) @name)
                (interface_declaration name: (type_identifier) @name)
            "#,
            imports: r#"
                (import_statement) @path
            "#,
            calls: r#"
                (call_expression function: (identifier) @name)
                (call_expression function: (member_expression property: (property_identifier) @name))
                (new_expression constructor: (identifier) @name)
            "#,
        },
    },
    LangInfo {
        name: "go",
        extensions: &["go"],
        language: || tree_sitter_go::LANGUAGE.into(),
        fn_kinds: &["function_declaration", "method_declaration"],
        fn_name_kinds: &["identifier", "field_identifier"],
        class_kinds: &["type_declaration", "method_declaration"],
        queries: LangQueries {
            functions: r#"
                (function_declaration name: (identifier) @name)
                (method_declaration name: (field_identifier) @name)
            "#,
            classes: r#"
                (type_declaration (type_spec name: (type_identifier) @name))
            "#,
            imports: r#"
                (import_declaration) @path
            "#,
            calls: r#"
                (call_expression function: (identifier) @name)
                (call_expression function: (selector_expression field: (field_identifier) @name))
            "#,
        },
    },
    LangInfo {
        name: "java",
        extensions: &["java"],
        language: || tree_sitter_java::LANGUAGE.into(),
        fn_kinds: &["method_declaration", "constructor_declaration"],
        fn_name_kinds: &["identifier"],
        class_kinds: &[
            "class_declaration",
            "interface_declaration",
            "enum_declaration",
        ],
        queries: LangQueries {
            functions: r#"
                (method_declaration name: (identifier) @name)
                (constructor_declaration name: (identifier) @name)
            "#,
            classes: r#"
                (class_declaration name: (identifier) @name)
                (interface_declaration name: (identifier) @name)
                (enum_declaration name: (identifier) @name)
            "#,
            imports: r#"
                (import_declaration) @path
            "#,
            calls: r#"
                (method_invocation name: (identifier) @name)
                (object_creation_expression type: (type_identifier) @name)
            "#,
        },
    },
    LangInfo {
        name: "kotlin",
        extensions: &["kt", "kts"],
        language: || tree_sitter_kotlin_ng::LANGUAGE.into(),
        fn_kinds: &["function_declaration"],
        fn_name_kinds: &["identifier"],
        class_kinds: &["class_declaration", "object_declaration"],
        queries: LangQueries {
            functions: r#"
                (function_declaration name: (identifier) @name)
            "#,
            classes: r#"
                (class_declaration name: (identifier) @name)
                (object_declaration name: (identifier) @name)
            "#,
            imports: r#"
                (import) @path
            "#,
            calls: r#"
                (call_expression (identifier) @name)
                (call_expression (navigation_expression (identifier) @name))
            "#,
        },
    },
    LangInfo {
        name: "swift",
        extensions: &["swift"],
        language: || tree_sitter_swift::LANGUAGE.into(),
        fn_kinds: &[
            "function_declaration",
            "init_declaration",
            "deinit_declaration",
        ],
        fn_name_kinds: &["simple_identifier"],
        class_kinds: &["class_declaration", "protocol_declaration"],
        queries: LangQueries {
            functions: r#"
                (function_declaration name: (simple_identifier) @name)
            "#,
            classes: r#"
                (class_declaration name: (type_identifier) @name)
                (class_declaration name: (user_type (type_identifier) @name))
                (protocol_declaration name: (type_identifier) @name)
                (protocol_declaration name: (user_type (type_identifier) @name))
            "#,
            imports: r#"
                (import_declaration) @path
            "#,
            calls: r#"
                (call_expression (simple_identifier) @name)
                (call_expression (navigation_expression suffix: (navigation_suffix suffix: (simple_identifier) @name)))
                (constructor_expression (user_type (type_identifier) @name))
            "#,
        },
    },
    LangInfo {
        name: "ruby",
        extensions: &["rb", "rake", "gemspec"],
        language: || tree_sitter_ruby::LANGUAGE.into(),
        fn_kinds: &["method", "singleton_method"],
        fn_name_kinds: &["identifier"],
        class_kinds: &["class", "module"],
        queries: LangQueries {
            functions: r#"
                (method name: (identifier) @name)
                (singleton_method name: (identifier) @name)
            "#,
            classes: r#"
                (class name: (constant) @name)
                (module name: (constant) @name)
            "#,
            imports: r#"
                (call
                  method: (identifier) @_method
                  (#match? @_method "^(require|require_relative|load)$")) @path
            "#,
            calls: r#"
                (call method: (identifier) @name)
                (call receiver: (constant) @name)
            "#,
        },
    },
    LangInfo {
        name: "elixir",
        extensions: &["ex", "exs"],
        language: || tree_sitter_elixir::LANGUAGE.into(),
        // Elixir's tree-sitter grammar represents `def`/`defmodule` as `call` nodes,
        // the same type as regular function calls. Empty fn_kinds/class_kinds disables
        // enclosing-context attribution in call graphs; semantic mode (function/module/import
        // listing) works fully via the queries below.
        fn_kinds: &[],
        fn_name_kinds: &[],
        class_kinds: &[],
        queries: LangQueries {
            functions: r#"
                (call
                  target: (identifier) @_ignore
                  (arguments
                    [
                      (identifier) @name
                      (call target: (identifier) @name)
                      (binary_operator
                        left: (call target: (identifier) @name)
                        operator: "when")
                    ])
                  (#any-of? @_ignore "def" "defp" "defdelegate" "defguard" "defguardp" "defmacro" "defmacrop" "defn" "defnp"))
            "#,
            classes: r#"
                (call
                  target: (identifier) @_ignore
                  (arguments (alias) @name)
                  (#any-of? @_ignore "defmodule" "defprotocol"))
            "#,
            imports: r#"
                (call
                  target: (identifier) @_method
                  (arguments . (_) @path)
                  (#any-of? @_method "import" "alias" "require" "use"))
            "#,
            calls: r#"
                (call target: (identifier) @name
                     (#not-any-of? @name
                          "def" "defdelegate" "defexception" "defguard" "defguardp"
                          "defimpl" "defmacro" "defmacrop" "defmodule" "defn" "defnp"
                          "defoverridable" "defp" "defprotocol" "defstruct"
                          "alias" "case" "cond" "else" "for" "if" "import" "quote"
                          "raise" "receive" "require" "reraise" "super" "throw"
                          "try" "unless" "unquote" "unquote_splicing" "use" "with"))
                (call target: (dot right: (identifier) @name))
                (binary_operator operator: "|>" right: (identifier) @name)
            "#,
        },
    },
];

pub fn lang_for_ext(ext: &str) -> Option<&'static LangInfo> {
    LANGUAGES.iter().find(|l| l.extensions.contains(&ext))
}
