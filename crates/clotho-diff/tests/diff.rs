//! Symbol-level diff tests: given before/after contents, the engine reports
//! which named symbols were added, removed, or modified — never patch text.

use clotho_diff::{diff_file, ChangeStatus};

fn find<'d>(
    diff: &'d clotho_diff::FileDiff,
    kind: &str,
    name: &str,
) -> &'d clotho_diff::SymbolChange {
    diff.symbols
        .iter()
        .find(|s| s.kind == kind && s.name == name)
        .unwrap_or_else(|| panic!("expected {kind} {name} in {:?}", diff.symbols))
}

#[test]
fn rust_symbol_diff_reports_add_modify_remove() {
    let old = r#"
pub struct Loom {
    threads: u32,
}

impl Loom {
    pub fn spin(&self) -> u32 {
        self.threads
    }

    pub fn unravel(&self) {}
}

fn helper() -> bool {
    true
}
"#;
    let new = r#"
pub struct Loom {
    threads: u32,
    tension: f64,
}

impl Loom {
    pub fn spin(&self) -> u32 {
        self.threads * 2
    }

    pub fn weave(&self) -> f64 {
        self.tension
    }
}

fn helper() -> bool {
    true
}
"#;
    let diff = diff_file("src/loom.rs", old.as_bytes(), new.as_bytes()).unwrap();
    assert_eq!(diff.language, Some("rust"));
    assert_eq!(diff.status, ChangeStatus::Modified);

    let strukt = find(&diff, "struct", "Loom");
    assert_eq!(strukt.status, ChangeStatus::Modified);
    let spin = find(&diff, "function", "Loom::spin");
    assert_eq!(spin.status, ChangeStatus::Modified);
    assert!(spin.old_lines.is_some() && spin.new_lines.is_some());
    assert_eq!(
        find(&diff, "function", "Loom::weave").status,
        ChangeStatus::Added
    );
    assert_eq!(
        find(&diff, "function", "Loom::unravel").status,
        ChangeStatus::Removed
    );
    // Untouched symbols stay out of the diff.
    assert!(!diff.symbols.iter().any(|s| s.name == "helper"));
}

#[test]
fn typescript_symbol_diff_covers_classes_and_arrow_functions() {
    let old = r#"
export class Spindle {
  turn(): number {
    return 1;
  }
}

export const fates = ["clotho"];

export function measure(): void {}
"#;
    let new = r#"
export class Spindle {
  turn(): number {
    return 2;
  }
  stop(): void {}
}

export const fates = ["clotho", "lachesis"];

export interface Thread {
  length: number;
}
"#;
    let diff = diff_file("src/spindle.ts", old.as_bytes(), new.as_bytes()).unwrap();
    assert_eq!(diff.language, Some("typescript"));

    assert_eq!(
        find(&diff, "method", "Spindle.turn").status,
        ChangeStatus::Modified
    );
    assert_eq!(
        find(&diff, "method", "Spindle.stop").status,
        ChangeStatus::Added
    );
    assert_eq!(
        find(&diff, "variable", "fates").status,
        ChangeStatus::Modified
    );
    assert_eq!(
        find(&diff, "function", "measure").status,
        ChangeStatus::Removed
    );
    assert_eq!(
        find(&diff, "interface", "Thread").status,
        ChangeStatus::Added
    );
}

#[test]
fn added_and_removed_files_diff_against_nothing() {
    let content = "pub fn woven() {}\n";
    let added = diff_file("new.rs", b"", content.as_bytes()).unwrap();
    assert_eq!(added.status, ChangeStatus::Added);
    assert_eq!(
        find(&added, "function", "woven").status,
        ChangeStatus::Added
    );
    assert!(find(&added, "function", "woven").old_lines.is_none());

    let removed = diff_file("old.rs", content.as_bytes(), b"").unwrap();
    assert_eq!(removed.status, ChangeStatus::Removed);
    assert_eq!(
        find(&removed, "function", "woven").status,
        ChangeStatus::Removed
    );
}

#[test]
fn unsupported_language_still_diffs_at_file_level() {
    let diff = diff_file("notes.md", b"# a\n", b"# b\n").unwrap();
    assert_eq!(diff.language, None);
    assert_eq!(diff.status, ChangeStatus::Modified);
    assert!(diff.symbols.is_empty());
}
