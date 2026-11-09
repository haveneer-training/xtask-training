//! Inventaire des trous du fil rouge (`cargo xtrain inventory`).
//!
//! Lancé depuis le corrigé, liste les blocs `SOLUTION-BEGIN` / `SOLUTION-END` des
//! [`Project::source_dirs`] : par étape, leur nombre, le volume du corrigé (lignes non
//! vides) et ce qu'en reçoit le stagiaire (une ligne `todo!()`, rien pour un trou vide) ;
//! puis un bloc par ligne, avec la fonction (ou l'`impl`, le `mod`…) qui le contient. Le
//! tout en Markdown, à coller dans un rapport de progression.

use crate::blocks::{Block, SOLUTION_BEGIN, SOLUTION_END, find_blocks};
use crate::files::{relative, source_files};
use crate::project::Project;
use crate::starter::ensure_reference;
use crate::update::scopes;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// Un trou du corrigé.
struct Hole {
    step: u8,
    path: String,
    line: usize,
    scope: String,
    reference: usize,
    starter: usize,
}

/// Affiche l'inventaire des trous du corrigé `root`.
pub fn inventory(p: &Project, root: &Path) -> Result<(), String> {
    ensure_reference(p, root)?;
    print!("{}", render(&holes(p, root)?));
    Ok(())
}

fn holes(p: &Project, root: &Path) -> Result<Vec<Hole>, String> {
    let mut holes = Vec::new();
    for path in source_files(p, root) {
        let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let blocks = find_blocks(&text, SOLUTION_BEGIN, SOLUTION_END);
        let key = relative(root, &path);
        for (block, scope) in blocks.iter().zip(scopes(&text, &blocks)) {
            holes.push(hole(&key, block, scope));
        }
    }
    holes.sort_by(|a, b| (a.step, &a.path, a.line).cmp(&(b.step, &b.path, b.line)));
    Ok(holes)
}

fn hole(path: &str, block: &Block, scope: String) -> Hole {
    Hole {
        step: block.step,
        path: path.to_string(),
        line: block.start + 1,
        scope,
        reference: block.body.iter().filter(|l| !l.trim().is_empty()).count(),
        starter: usize::from(!block.blank),
    }
}

fn render(holes: &[Hole]) -> String {
    let mut out = String::new();
    out.push_str("| Étape | Blocs | Corrigé (lignes non vides) | Squelette (lignes) |\n");
    out.push_str("|---:|---:|---:|---:|\n");
    let mut steps: BTreeMap<u8, (usize, usize, usize)> = BTreeMap::new();
    for hole in holes {
        let entry = steps.entry(hole.step).or_default();
        *entry = (
            entry.0 + 1,
            entry.1 + hole.reference,
            entry.2 + hole.starter,
        );
    }
    for (step, (count, reference, starter)) in &steps {
        out.push_str(&format!("| {step} | {count} | {reference} | {starter} |\n"));
    }
    out.push_str("\n| Étape | Fichier:ligne | Fonction / bloc | Corrigé → stagiaire |\n");
    out.push_str("|---:|---|---|---:|\n");
    for hole in holes {
        let scope = if hole.scope.is_empty() {
            "définitions à écrire".to_string()
        } else {
            format!("`{}`", hole.scope)
        };
        out.push_str(&format!(
            "| {} | `{}:{}` | {scope} | {} → {} |\n",
            hole.step, hole.path, hole.line, hole.reference, hole.starter
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holes_are_counted_per_step_and_named_by_scope() {
        let text = "\
impl Mesh {
    fn area(&self) -> f64 {
        // TODO-STEP:2 la surface
        // SOLUTION-BEGIN
        let a = 1.0;

        a * 2.0
        // SOLUTION-END
    }
}
// TODO-STEP:1 les types
// SOLUTION-BEGIN vide
struct Cell;
// SOLUTION-END
";
        let blocks = find_blocks(text, SOLUTION_BEGIN, SOLUTION_END);
        let holes: Vec<Hole> = blocks
            .iter()
            .zip(scopes(text, &blocks))
            .map(|(b, s)| hole("src/lib.rs", b, s))
            .collect();
        let table = render(&holes);
        assert!(
            table.contains("| 2 | `src/lib.rs:4` | `impl Mesh::area` | 2 → 1 |"),
            "{table}"
        );
        assert!(
            table.contains("| 1 | `src/lib.rs:12` | définitions à écrire | 1 → 0 |"),
            "{table}"
        );
        assert!(table.contains("| 2 | 1 | 2 | 1 |"), "{table}");
    }
}
