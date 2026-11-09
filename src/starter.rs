//! Génération du dossier de travail à partir du corrigé (`cargo xtrain start`).

use crate::blocks::{
    BLANK_NOTE, FILL_NOTE, REGION_BEGIN, REGION_END, SOLUTION_BEGIN, SOLUTION_END, find_blocks,
    hole_body,
};
use crate::exercises::{statement, write_statements};
use crate::files::{copy_tree, markdown_files, relative, rust_files_in, source_files};
use crate::project::{Project, TOOL_DIR};
use crate::steps::{REFERENCE, rewrite_default, set_default_feature};
use std::fs;
use std::path::Path;

/// Nom du dossier de travail engendré, à la racine du dépôt.
pub const WORKDIR: &str = "travail";

/// Marqueur ouvrant d'un passage Markdown qui ne vaut que dans le dépôt corrigé.
const CORRIGE_BEGIN: &str = "<!-- CORRIGE-ONLY-BEGIN -->";

/// Marqueur fermant d'un passage Markdown qui ne vaut que dans le dépôt corrigé.
const CORRIGE_END: &str = "<!-- CORRIGE-ONLY-END -->";

/// Première ligne d'un fichier de [`Project::corrige_trees`] qui n'est pas livré du tout (un
/// fichier de corrigés entier, le module qui le déclare étant dans un bloc).
const CORRIGE_ONLY_FILE: &str = "// CORRIGE-ONLY-FILE";

/// Refuse de continuer si `root` n'est pas le dépôt du corrigé.
pub fn ensure_reference(p: &Project, root: &Path) -> Result<(), String> {
    // Le dépôt du corrigé est la source : sans ses marqueurs, il n'y a rien à trouer.
    // Ces commandes lancées depuis un dossier de travail ne produiraient qu'une copie
    // sans exercices, ce qui serait une fausse bonne surprise.
    let is_reference = source_files(p, root).iter().any(|path| {
        fs::read_to_string(path)
            .map(|t| t.contains(SOLUTION_BEGIN))
            .unwrap_or(false)
    });
    if is_reference {
        Ok(())
    } else {
        Err(
            "ce dépôt ne contient aucun bloc de référence : c'est déjà un dossier de \
                    travail.\nLancez cette commande depuis le dépôt du corrigé, ou \
                    `cargo xtrain status` pour voir où vous en êtes."
                .to_string(),
        )
    }
}

/// Engendre le dépôt de travail à partir du corrigé.
pub fn make_starter(p: &Project, root: &Path, out: &Path, force: bool) -> Result<(), String> {
    ensure_reference(p, root)?;

    // Ne jamais détruire le travail de quelqu'un sans le lui demander.
    if out.exists() && fs::read_dir(out).map(|d| d.count() > 0).unwrap_or(false) {
        if !force {
            return Err(format!(
                "{} existe déjà et n'est pas vide.\nSi c'est votre dossier de travail, \
                 il contient ce que vous avez écrit : ne le régénérez pas.\nPour le \
                 remplacer malgré tout — et perdre son contenu — ajoutez --force.\nPour \
                 y reporter les dernières modifications du corrigé, utilisez \
                 `cargo xtrain update`.",
                out.display()
            ));
        }
        fs::remove_dir_all(out).map_err(|e| format!("{} : {e}", out.display()))?;
    }
    fs::create_dir_all(out).map_err(|e| e.to_string())?;

    let holes = generate(p, root, out)?;

    let shown = out.strip_prefix(root).unwrap_or(out);
    println!(
        "Dossier de travail créé : {} ({holes} trous à combler).\n",
        shown.display()
    );
    println!("  cd {}", shown.display());
    println!("  cargo test           # tests rouges : l'étape 0 vous attend");
    println!("  cargo xtrain status  # à tout moment, pour savoir où vous en êtes\n");
    println!("Le déroulement d'une étape est expliqué dans ETAPES.md.");
    Ok(())
}

/// Écrit dans `out`, supposé vide, le dossier de travail tiré du corrigé `root`, et
/// renvoie le nombre de trous.
pub fn generate(p: &Project, root: &Path, out: &Path) -> Result<usize, String> {
    let mut reference = String::new();
    let mut holes = 0;

    for entry in p.starter_entries {
        copy_tree(&root.join(entry), &out.join(entry))?;
    }
    for path in p.corrige_only_paths {
        let path = out.join(path);
        if path.exists() {
            fs::remove_dir_all(&path).map_err(|e| format!("{} : {e}", path.display()))?;
        }
    }

    // Les blocs CORRIGE-ONLY (« comment engendrer travail/ », par exemple, dans le
    // README) ne valent que dans ce dépôt-ci : on les retire de la documentation copiée
    // avant de la livrer au dossier de travail.
    let mut docs = Vec::new();
    markdown_files(out, &mut docs);
    for path in docs {
        let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        if !text.contains(CORRIGE_BEGIN) {
            continue;
        }
        let stripped = strip_corrige_blocks(&text, &path)?;
        fs::write(&path, stripped).map_err(|e| e.to_string())?;
    }

    // Les exercices courts du dépôt sont leurs corrigés : en tirer les énoncés.
    write_statements(p, out)?;
    // De même pour le code hors fil rouge qui porte ses corrigés
    // ([`Project::corrige_trees`]).
    write_corrige_statements(p, out)?;

    // Trouer les sources et mettre les corps de côté.
    for path in source_files(p, out) {
        let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let blocks = find_blocks(&text, SOLUTION_BEGIN, SOLUTION_END);
        if blocks.is_empty() {
            continue;
        }
        let key = relative(out, &path);

        for block in &blocks {
            reference.push_str(&format!(
                "@@ {key} @@ {} @@ {}\n",
                block.step, block.ordinal
            ));
            for line in &block.body {
                reference.push_str(line);
                reference.push('\n');
            }
            reference.push_str("@@\n");
        }

        let lines: Vec<&str> = text.lines().collect();
        let mut new_lines: Vec<String> = Vec::new();
        let mut i = 0;
        while i < lines.len() {
            match blocks.iter().find(|b| b.start == i) {
                Some(block) => {
                    let note = if block.blank { BLANK_NOTE } else { FILL_NOTE };
                    new_lines.push(format!(
                        "{}{REGION_BEGIN}{} — {note}",
                        block.indent, block.step
                    ));
                    new_lines.extend(hole_body(block));
                    new_lines.push(format!("{}{REGION_END}{}", block.indent, block.step));
                    holes += 1;
                    i = block.end + 1;
                }
                None => {
                    new_lines.push(humanize_todo(lines[i]));
                    i += 1;
                }
            }
        }
        fs::write(&path, new_lines.join("\n") + "\n").map_err(|e| e.to_string())?;
    }

    fs::create_dir_all(out.join(TOOL_DIR)).map_err(|e| e.to_string())?;
    fs::write(out.join(REFERENCE), reference).map_err(|e| e.to_string())?;

    // Le dépôt de travail démarre à l'étape 0, ce qui retire du même coup la feature
    // sentinelle du corrigé (`step{last_step + 1}`) : elle y désactive les `allow`
    // conditionnels posés sur les avertissements des trous, et ne doit jamais être active
    // dans un dossier de travail. Les autres manifestes qui la portent la perdent aussi.
    set_default_feature(p, out, 0)?;
    for manifest in p.sentinel_manifests {
        rewrite_default(&out.join(manifest), "[]")?;
    }

    Ok(holes)
}

/// Sous `out`, remplace chaque fichier Rust de [`Project::corrige_trees`] par son énoncé,
/// et supprime ceux marqués [`CORRIGE_ONLY_FILE`].
fn write_corrige_statements(p: &Project, out: &Path) -> Result<(), String> {
    for tree in p.corrige_trees {
        for path in rust_files_in(&out.join(tree)) {
            let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
            let key = relative(out, &path);
            if text.starts_with(CORRIGE_ONLY_FILE) {
                fs::remove_file(&path).map_err(|e| format!("{key} : {e}"))?;
            } else if text.contains(SOLUTION_BEGIN) {
                let statement = statement(&text).map_err(|e| format!("{key} : {e}"))?;
                fs::write(&path, statement).map_err(|e| format!("{key} : {e}"))?;
            }
        }
    }
    Ok(())
}

/// Retire les blocs `CORRIGE_BEGIN`/`CORRIGE_END` d'un texte Markdown.
///
/// Ces blocs ne valent que dans le dépôt corrigé (« comment engendrer `travail/` », par
/// exemple, dans le README) : le dossier de travail ne doit jamais les voir. Un marqueur
/// ouvrant sans fermant est une erreur d'écriture du corrigé — on s'arrête plutôt que de
/// publier un fichier tronqué en silence, même discipline que [`find_blocks`].
fn strip_corrige_blocks(text: &str, path: &Path) -> Result<String, String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut kept: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim() == CORRIGE_BEGIN {
            let start = i;
            let Some(end) = (start + 1..lines.len()).find(|&k| lines[k].trim() == CORRIGE_END)
            else {
                return Err(format!(
                    "{} : {CORRIGE_BEGIN} sans {CORRIGE_END} (ligne {})",
                    path.display(),
                    start + 1
                ));
            };
            i = end + 1;
            continue;
        }
        kept.push(lines[i]);
        i += 1;
    }

    // Fusionner les lignes vides laissées par un bloc retiré, pour ne pas publier de
    // double blanc à sa place.
    let mut merged: Vec<&str> = Vec::new();
    for line in kept {
        if line.is_empty() && merged.last() == Some(&"") {
            continue;
        }
        merged.push(line);
    }
    Ok(merged.join("\n") + "\n")
}

/// Transforme le marqueur machine du corrigé en consigne lisible.
///
/// `// TODO-STEP:3 Écrire l'en-tête` devient `// À FAIRE (étape 3) Écrire l'en-tête`.
fn humanize_todo(line: &str) -> String {
    let Some((before, after)) = line.split_once("TODO-STEP:") else {
        return line.to_string();
    };
    let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return line.to_string();
    }
    let rest = &after[digits.len()..];
    format!("{before}À FAIRE (étape {digits}){rest}")
}
