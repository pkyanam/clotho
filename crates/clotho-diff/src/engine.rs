//! The tree-sitter-backed symbol diff engine.
//!
//! Given full before/after file contents, produce what changed at the symbol
//! level — "function `VcsEngine::commit` was modified" — instead of patch
//! text. One structured object serves both audiences (docs/prd.md §2): the
//! agent-facing MCP `diff_symbol` tool and the future human PR view.
//!
//! Prototype scope: Rust and TypeScript, detected by file extension. Files in
//! other languages still diff (added/modified/deleted status), they just
//! carry no symbol breakdown.

use tree_sitter::{Node, Parser};

#[derive(Debug, thiserror::Error)]
pub enum DiffError {
    #[error("parser error: {0}")]
    Parser(String),
    #[error("file {0:?} is not valid utf-8: {1}")]
    NotUtf8(String, String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeStatus {
    Added,
    Modified,
    Removed,
}

#[derive(Debug)]
pub struct SymbolChange {
    /// Qualified name, e.g. `VcsEngine::commit` or `GatewayConfig`.
    pub name: String,
    /// Language-level kind: "function", "struct", "class", "method", ...
    pub kind: String,
    pub status: ChangeStatus,
    /// 1-based line ranges on each side; `None` when absent on that side.
    pub old_lines: Option<(u32, u32)>,
    pub new_lines: Option<(u32, u32)>,
}

#[derive(Debug)]
pub struct FileDiff {
    pub path: String,
    /// Detected language; `None` when unsupported (no symbol breakdown).
    pub language: Option<&'static str>,
    pub status: ChangeStatus,
    pub symbols: Vec<SymbolChange>,
}

/// A symbol extracted from one side of the diff.
struct Symbol {
    name: String,
    kind: &'static str,
    /// Source text of the whole symbol, used to detect modification.
    text: String,
    start_line: u32,
    end_line: u32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Language {
    Rust,
    /// Plain TypeScript and TSX use separate grammars (TSX changes how `<`
    /// parses) but share the same symbol shapes, so one collector serves both.
    TypeScript {
        tsx: bool,
    },
}

impl Language {
    fn detect(path: &str) -> Option<Self> {
        let ext = path.rsplit_once('.')?.1;
        match ext {
            "rs" => Some(Self::Rust),
            "ts" | "mts" | "cts" => Some(Self::TypeScript { tsx: false }),
            "tsx" => Some(Self::TypeScript { tsx: true }),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::TypeScript { .. } => "typescript",
        }
    }

    fn grammar(self) -> tree_sitter::Language {
        match self {
            Self::Rust => tree_sitter_rust::LANGUAGE.into(),
            Self::TypeScript { tsx: false } => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Self::TypeScript { tsx: true } => tree_sitter_typescript::LANGUAGE_TSX.into(),
        }
    }
}

/// Diff one file's before/after contents at the symbol level.
pub fn diff_file(path: &str, old: &[u8], new: &[u8]) -> Result<FileDiff, DiffError> {
    let status = if old.is_empty() {
        ChangeStatus::Added
    } else if new.is_empty() {
        ChangeStatus::Removed
    } else {
        ChangeStatus::Modified
    };
    let Some(language) = Language::detect(path) else {
        return Ok(FileDiff {
            path: path.to_string(),
            language: None,
            status,
            symbols: Vec::new(),
        });
    };

    let old_symbols = extract_symbols(path, language, old)?;
    let new_symbols = extract_symbols(path, language, new)?;

    // Match symbols across sides by (kind, qualified name).
    let mut symbols = Vec::new();
    for old_sym in &old_symbols {
        match new_symbols
            .iter()
            .find(|n| n.name == old_sym.name && n.kind == old_sym.kind)
        {
            Some(new_sym) if new_sym.text == old_sym.text => {}
            Some(new_sym) => symbols.push(SymbolChange {
                name: old_sym.name.clone(),
                kind: old_sym.kind.to_string(),
                status: ChangeStatus::Modified,
                old_lines: Some((old_sym.start_line, old_sym.end_line)),
                new_lines: Some((new_sym.start_line, new_sym.end_line)),
            }),
            None => symbols.push(SymbolChange {
                name: old_sym.name.clone(),
                kind: old_sym.kind.to_string(),
                status: ChangeStatus::Removed,
                old_lines: Some((old_sym.start_line, old_sym.end_line)),
                new_lines: None,
            }),
        }
    }
    for new_sym in &new_symbols {
        if !old_symbols
            .iter()
            .any(|o| o.name == new_sym.name && o.kind == new_sym.kind)
        {
            symbols.push(SymbolChange {
                name: new_sym.name.clone(),
                kind: new_sym.kind.to_string(),
                status: ChangeStatus::Added,
                old_lines: None,
                new_lines: Some((new_sym.start_line, new_sym.end_line)),
            });
        }
    }

    Ok(FileDiff {
        path: path.to_string(),
        language: Some(language.name()),
        status,
        symbols,
    })
}

fn extract_symbols(
    path: &str,
    language: Language,
    content: &[u8],
) -> Result<Vec<Symbol>, DiffError> {
    if content.is_empty() {
        return Ok(Vec::new());
    }
    let source = std::str::from_utf8(content)
        .map_err(|e| DiffError::NotUtf8(path.to_string(), e.to_string()))?;
    let mut parser = Parser::new();
    parser
        .set_language(&language.grammar())
        .map_err(|e| DiffError::Parser(e.to_string()))?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| DiffError::Parser(format!("failed to parse {path}")))?;

    let mut symbols = Vec::new();
    collect(language, tree.root_node(), source, "", &mut symbols);
    Ok(symbols)
}

/// Walk the AST collecting named symbols, recursing into containers (Rust
/// `impl`/`mod` blocks, TypeScript classes/namespaces) with a qualified-name
/// prefix.
fn collect(language: Language, node: Node, source: &str, prefix: &str, out: &mut Vec<Symbol>) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match language {
            Language::Rust => collect_rust(child, source, prefix, out),
            Language::TypeScript { .. } => collect_ts(language, child, source, prefix, out),
        }
    }
}

fn push(out: &mut Vec<Symbol>, node: Node, source: &str, kind: &'static str, name: String) {
    out.push(Symbol {
        name,
        kind,
        text: source[node.byte_range()].to_string(),
        start_line: node.start_position().row as u32 + 1,
        end_line: node.end_position().row as u32 + 1,
    });
}

fn qualified(prefix: &str, sep: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}{sep}{name}")
    }
}

fn node_text<'s>(node: Option<Node>, source: &'s str) -> Option<&'s str> {
    node.map(|n| &source[n.byte_range()])
}

fn collect_rust(node: Node, source: &str, prefix: &str, out: &mut Vec<Symbol>) {
    let name_field = node_text(node.child_by_field_name("name"), source);
    match node.kind() {
        "function_item" | "function_signature_item" => {
            if let Some(name) = name_field {
                push(out, node, source, "function", qualified(prefix, "::", name));
            }
        }
        "struct_item" => rust_item(node, source, prefix, "struct", name_field, out),
        "enum_item" => rust_item(node, source, prefix, "enum", name_field, out),
        "union_item" => rust_item(node, source, prefix, "union", name_field, out),
        "trait_item" => {
            if let Some(name) = name_field {
                let qualified_name = qualified(prefix, "::", name);
                push(out, node, source, "trait", qualified_name.clone());
                collect(Language::Rust, node, source, &qualified_name, out);
            }
        }
        "type_item" => rust_item(node, source, prefix, "type", name_field, out),
        "const_item" => rust_item(node, source, prefix, "const", name_field, out),
        "static_item" => rust_item(node, source, prefix, "static", name_field, out),
        "macro_definition" => rust_item(node, source, prefix, "macro", name_field, out),
        "mod_item" => {
            if let Some(name) = name_field {
                collect(
                    Language::Rust,
                    node,
                    source,
                    &qualified(prefix, "::", name),
                    out,
                );
            }
        }
        "impl_item" => {
            // Functions inside `impl Type` / `impl Trait for Type` are
            // reported as `Type::method`, matching how humans name them.
            if let Some(type_name) = node_text(node.child_by_field_name("type"), source) {
                collect(
                    Language::Rust,
                    node,
                    source,
                    &qualified(prefix, "::", type_name),
                    out,
                );
            }
        }
        "declaration_list" => collect(Language::Rust, node, source, prefix, out),
        _ => {}
    }
}

fn rust_item(
    node: Node,
    source: &str,
    prefix: &str,
    kind: &'static str,
    name: Option<&str>,
    out: &mut Vec<Symbol>,
) {
    if let Some(name) = name {
        push(out, node, source, kind, qualified(prefix, "::", name));
    }
}

fn collect_ts(language: Language, node: Node, source: &str, prefix: &str, out: &mut Vec<Symbol>) {
    let name_field = node_text(node.child_by_field_name("name"), source);
    match node.kind() {
        // `export function f() {}` wraps the declaration — unwrap it.
        "export_statement" => collect(language, node, source, prefix, out),
        "function_declaration" | "generator_function_declaration" => {
            if let Some(name) = name_field {
                push(out, node, source, "function", qualified(prefix, ".", name));
            }
        }
        "class_declaration" | "abstract_class_declaration" => {
            if let Some(name) = name_field {
                let qualified_name = qualified(prefix, ".", name);
                push(out, node, source, "class", qualified_name.clone());
                collect(language, node, source, &qualified_name, out);
            }
        }
        "interface_declaration" => {
            if let Some(name) = name_field {
                push(out, node, source, "interface", qualified(prefix, ".", name));
            }
        }
        "type_alias_declaration" => {
            if let Some(name) = name_field {
                push(out, node, source, "type", qualified(prefix, ".", name));
            }
        }
        "enum_declaration" => {
            if let Some(name) = name_field {
                push(out, node, source, "enum", qualified(prefix, ".", name));
            }
        }
        "method_definition" => {
            if let Some(name) = name_field {
                push(out, node, source, "method", qualified(prefix, ".", name));
            }
        }
        "lexical_declaration" | "variable_declaration" => {
            // `const f = () => {}` and friends: one symbol per declarator.
            let mut cursor = node.walk();
            for declarator in node.named_children(&mut cursor) {
                if declarator.kind() == "variable_declarator" {
                    if let Some(name) = node_text(declarator.child_by_field_name("name"), source) {
                        push(out, node, source, "variable", qualified(prefix, ".", name));
                    }
                }
            }
        }
        "internal_module" => {
            // `namespace N { ... }`
            if let Some(name) = name_field {
                collect(language, node, source, &qualified(prefix, ".", name), out);
            }
        }
        "class_body" | "statement_block" => collect(language, node, source, prefix, out),
        _ => {}
    }
}
