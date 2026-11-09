//! Report des modifications du corrigé dans un dossier de travail (`cargo xtrain update`).

use crate::blocks::{Block, REGION_BEGIN, REGION_END, find_blocks, is_empty, rewrite};
use crate::exercises::{DELIVERED, carry_over};
use crate::files::{relative, source_files};
use crate::project::{Project, TOOL_DIR};
use crate::starter::{ensure_reference, generate};
use crate::steps::{REFERENCE, current_step, load_reference, set_default_feature};
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

/// Reporte les dernières modifications du corrigé dans un dossier de travail existant.
///
/// Sert pendant la mise au point des exercices : le dossier de travail est engendré à
/// nouveau, à côté, puis ce que le stagiaire a écrit y est replacé avant qu'il ne remplace
/// l'ancien. Sont conservés : les trous remplis (ceux remplis par `solve` reprennent le
/// nouveau corrigé), les exercices courts modifiés (voir [`carry_over`]), l'étape
/// courante, et tout ce qui n'est pas tiré du corrigé (`target/`, `data/`, un éventuel
/// `.git/`). Tout le reste — documentation, tests, code hors des trous — est remplacé.
pub fn update(p: &Project, root: &Path, out: &Path) -> Result<(), String> {
    ensure_reference(p, root)?;
    let migrated = migrate_legacy_tool_dir(out)?;
    let result = update_checked(p, root, out);
    if migrated && result.is_err() {
        // Rien n'a été remplacé : rendre au dossier son ancienne forme, qui fonctionne
        // encore avec son `cargo xtask`.
        let _ = fs::rename(out.join(TOOL_DIR), out.join(LEGACY_TOOL_DIR));
    }
    result
}

/// Ancien nom de [`TOOL_DIR`], du temps où l'outillage s'appelait `cargo xtask`.
const LEGACY_TOOL_DIR: &str = "xtask";

/// Un dossier de travail engendré par l'ancien `cargo xtask` range ses blocs de référence
/// et ses énoncés livrés sous `xtask/` : les déplacer sous [`TOOL_DIR`], où `update` les
/// attend. La crate `xtask` qui les accompagne est remplacée par la mise à jour, avec le
/// manifeste du workspace qui la déclarait.
fn migrate_legacy_tool_dir(out: &Path) -> Result<bool, String> {
    let (legacy, current) = (out.join(LEGACY_TOOL_DIR), out.join(TOOL_DIR));
    if !legacy.join("reference.txt").is_file() || current.exists() {
        return Ok(false);
    }
    fs::rename(&legacy, &current).map_err(|e| format!("{} : {e}", legacy.display()))?;
    println!("  {LEGACY_TOOL_DIR}/ → {TOOL_DIR}/ : dossier de travail de l'ancien `cargo xtask`");
    Ok(true)
}

fn update_checked(p: &Project, root: &Path, out: &Path) -> Result<(), String> {
    if !out.join(REFERENCE).is_file() {
        return Err(format!(
            "{} n'est pas un dossier de travail ({REFERENCE} introuvable).\nCréez-le avec \
             `cargo xtrain start`.",
            out.display()
        ));
    }
    let out = fs::canonicalize(out).map_err(|e| format!("{} : {e}", out.display()))?;
    let name = out
        .file_name()
        .ok_or_else(|| format!("{} : chemin sans nom de dossier", out.display()))?;
    // À côté du dossier de travail, pour que les renommages restent sur le même disque.
    let staging = out.with_file_name(format!(".{}-update", name.to_string_lossy()));
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|e| format!("{} : {e}", staging.display()))?;
    }
    fs::create_dir_all(&staging).map_err(|e| e.to_string())?;

    let result = update_into(p, root, &out, &staging);
    let _ = fs::remove_dir_all(&staging);
    result
}

fn update_into(p: &Project, root: &Path, out: &Path, staging: &Path) -> Result<(), String> {
    let holes = generate(p, root, staging)?;
    let old_reference = load_reference(out)?;
    let new_reference = load_reference(staging)?;
    let mut kept = 0;
    let mut lost = Vec::new();

    for path in source_files(p, staging) {
        let key = relative(staging, &path);
        let Ok(old_text) = fs::read_to_string(out.join(&key)) else {
            continue;
        };
        let new_text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let (merged, changed, gone) =
            merge(&key, &old_text, &new_text, &old_reference, &new_reference);
        lost.extend(gone);
        if changed > 0 {
            fs::write(&path, merged).map_err(|e| e.to_string())?;
            println!("  {key} : {changed} bloc(s) conservé(s)");
            kept += changed;
        }
    }
    for path in source_files(p, out) {
        let key = relative(out, &path);
        if staging.join(&key).exists() {
            continue;
        }
        let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        if find_blocks(&text, REGION_BEGIN, REGION_END)
            .iter()
            .any(|b| !is_empty(b))
        {
            lost.push(format!("{key} : ce fichier n'existe plus dans le corrigé"));
        }
    }
    let exercises = carry_over(p, out, staging)?;
    lost.extend(exercises.lost);

    // Un remplissage perdu en silence serait la pire issue : on s'arrête avant d'avoir
    // touché au dossier de travail.
    if !lost.is_empty() {
        return Err(format!(
            "mise à jour abandonnée, du travail serait perdu :\n  {}\nRecopiez-le \
             ailleurs, rouvrez ces trous (`cargo xtrain reset <n>` dans le dossier de \
             travail) ou supprimez ces exercices, puis relancez.",
            lost.join("\n  ")
        ));
    }

    let step = current_step(p, out).unwrap_or(0);
    set_default_feature(p, staging, step)?;

    for &entry in p.starter_entries.iter().chain(&[REFERENCE, DELIVERED]) {
        let (from, to) = (staging.join(entry), out.join(entry));
        if to.is_dir() {
            fs::remove_dir_all(&to).map_err(|e| format!("{} : {e}", to.display()))?;
        } else if to.exists() {
            fs::remove_file(&to).map_err(|e| format!("{} : {e}", to.display()))?;
        }
        if from.exists() {
            fs::rename(&from, &to).map_err(|e| format!("{} : {e}", to.display()))?;
        }
    }

    for line in &exercises.kept {
        println!("  {line}");
    }
    let shown = out.strip_prefix(root).unwrap_or(out);
    println!(
        "Dossier de travail mis à jour : {} ({holes} trous, {kept} rempli(s) conservé(s), \
         étape {step}).",
        shown.display()
    );
    Ok(())
}

/// Corps de référence des blocs, par `(fichier, numéro d'ordre)` — voir [`load_reference`].
type Reference = BTreeMap<(String, usize), Vec<String>>;

/// Replace dans `new_text`, fichier `key` du nouveau dossier de travail, les trous remplis
/// de `old_text`, le même fichier dans l'ancien.
///
/// Renvoie le texte fusionné, le nombre de blocs repris et la liste des trous remplis qui
/// n'ont plus de place dans le nouveau texte.
fn merge(
    key: &str,
    old_text: &str,
    new_text: &str,
    old_reference: &Reference,
    new_reference: &Reference,
) -> (String, usize, Vec<String>) {
    let old_blocks = find_blocks(old_text, REGION_BEGIN, REGION_END);
    let new_blocks = find_blocks(new_text, REGION_BEGIN, REGION_END);

    // Les blocs se correspondent par leur étape et la fonction qui les contient (voir
    // [`block_keys`]) : un trou ajouté, retiré ou déplacé ailleurs ne décale pas les autres.
    let old_keys = block_keys(old_text, &old_blocks);
    let new_keys = block_keys(new_text, &new_blocks);
    let old_by_key: BTreeMap<&BlockKey, &Block> = old_keys.iter().zip(&old_blocks).collect();
    let mut matched = BTreeSet::new();

    let (merged, changed) = rewrite(new_text, &new_blocks, |b| {
        let id = &new_keys[b.ordinal];
        let old = old_by_key.get(id)?;
        matched.insert(id);
        if is_empty(old) {
            return None;
        }
        // Bloc rempli par `solve` : il suit le corrigé, nouvelle version comprise.
        if old_reference.get(&(key.to_string(), old.ordinal)) == Some(&old.body) {
            return new_reference.get(&(key.to_string(), b.ordinal)).cloned();
        }
        Some(old.body.clone())
    });
    let lost = old_by_key
        .iter()
        .filter(|(id, old)| !matched.contains(**id) && !is_empty(old))
        .map(|((_, scope, _), old)| {
            let place = if scope.is_empty() {
                String::new()
            } else {
                format!(", dans {scope}")
            };
            format!(
                "{key} ligne {} (étape {}{place}) : ce trou n'existe plus dans le corrigé",
                old.start + 1,
                old.step
            )
        })
        .collect();
    (merged, changed, lost)
}

/// Identité d'un bloc d'un texte à l'autre : `(étape, portée, rang parmi les blocs de même
/// étape et de même portée)`. La portée est le chemin des `mod`, `impl`, `trait` et `fn`
/// qui contiennent le bloc — `impl From<MeshError> for Error::from` —, vide hors de tout.
type BlockKey = (u8, String, usize);

/// La clé de chaque bloc, dans l'ordre des blocs.
///
/// La portée rend la clé stable quand un trou de la même étape apparaît plus
/// haut dans le fichier — le cas typique d'un exercice découpé en fonctions auxiliaires.
/// Un rang seul décalerait alors toutes les réponses en silence. Une fonction renommée
/// (ou un `impl` réécrit), elle, perd ses trous : la mise à jour s'arrête au lieu de les égarer.
fn block_keys(text: &str, blocks: &[Block]) -> Vec<BlockKey> {
    let mut seen: BTreeMap<(u8, String), usize> = BTreeMap::new();
    blocks
        .iter()
        .zip(scopes(text, blocks))
        .map(|(b, scope)| {
            let rank = seen.entry((b.step, scope.clone())).or_insert(0);
            *rank += 1;
            (b.step, scope, *rank - 1)
        })
        .collect()
}

/// Où en est la lecture du texte, d'une ligne à l'autre.
enum Lexer {
    Code,
    Str,
    /// Chaîne brute `r#"…"#`, avec son nombre de `#`.
    RawStr(usize),
    /// Commentaire `/* … */`, avec sa profondeur d'imbrication.
    Comment(usize),
}

/// Pour chaque bloc, sa portée (voir [`BlockKey`]).
///
/// Suit les accolades du texte, en sautant chaînes, caractères et commentaires : chaque
/// `{` ouvrant un `mod`, un `impl`, un `trait` ou une fonction porte une étiquette, les
/// autres (`if`, `match`, fermeture…) n'en portent pas. Le corps des blocs est sauté aussi : il vient du stagiaire et peut
/// être à moitié écrit, il ne doit pas déplacer les accolades du reste du fichier.
pub(crate) fn scopes(text: &str, blocks: &[Block]) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut names = Vec::with_capacity(blocks.len());
    let mut open: Vec<Option<String>> = Vec::new();
    let mut header = String::new();
    let mut state = Lexer::Code;
    let mut i = 0;

    while i < lines.len() {
        if let Some(block) = blocks.iter().find(|b| b.start == i) {
            let path: Vec<&str> = open.iter().flatten().map(String::as_str).collect();
            names.push(path.join("::"));
            i = block.end + 1;
            continue;
        }
        let chars: Vec<char> = lines[i].chars().collect();
        let mut k = 0;
        while k < chars.len() {
            let c = chars[k];
            let next = chars.get(k + 1).copied();
            match state {
                Lexer::Str => match c {
                    '\\' => k += 1,
                    '"' => state = Lexer::Code,
                    _ => {}
                },
                Lexer::RawStr(hashes) => {
                    if c == '"'
                        && chars[k + 1..].iter().take_while(|&&h| h == '#').count() >= hashes
                    {
                        k += hashes;
                        state = Lexer::Code;
                    }
                }
                Lexer::Comment(depth) => match (c, next) {
                    ('*', Some('/')) => {
                        k += 1;
                        state = if depth == 1 {
                            Lexer::Code
                        } else {
                            Lexer::Comment(depth - 1)
                        };
                    }
                    ('/', Some('*')) => {
                        k += 1;
                        state = Lexer::Comment(depth + 1);
                    }
                    _ => {}
                },
                Lexer::Code => match (c, next) {
                    ('/', Some('/')) => break,
                    ('/', Some('*')) => {
                        k += 1;
                        state = Lexer::Comment(1);
                    }
                    ('"', _) => state = Lexer::Str,
                    ('r', Some('"' | '#'))
                        if !chars[..k]
                            .last()
                            .is_some_and(|&p| p.is_alphanumeric() || p == '_') =>
                    {
                        let hashes = chars[k + 1..].iter().take_while(|&&h| h == '#').count();
                        if chars.get(k + 1 + hashes) == Some(&'"') {
                            k += 1 + hashes;
                            state = Lexer::RawStr(hashes);
                        } else {
                            header.push(c);
                        }
                    }
                    // `'{'`, `'\''` : un caractère ; `'a` : une durée de vie, sans effet.
                    ('\'', Some('\\')) => {
                        k += 2;
                        while k < chars.len() && chars[k] != '\'' {
                            k += 1;
                        }
                    }
                    ('\'', Some(_)) if chars.get(k + 2) == Some(&'\'') => k += 2,
                    ('{', _) => {
                        open.push(scope_label(&header));
                        header.clear();
                    }
                    ('}', _) => {
                        open.pop();
                        header.clear();
                    }
                    (';', _) => header.clear(),
                    _ => header.push(c),
                },
            }
            k += 1;
        }
        header.push(' ');
        i += 1;
    }
    names
}

/// L'étiquette de l'accolade que `header` précède, si elle en mérite une.
///
/// Pour une fonction, son nom : l'identifiant qui suit le dernier mot `fn`. Préfixes
/// (`pub(crate)`, `const`, `async`, `unsafe`, `extern "C"`), attributs et clauses `where`
/// n'y changent rien ; un type `fn(…)` n'est pas un mot `fn` à lui seul. Pour un `mod`, un
/// `impl` ou un `trait`, l'en-tête à partir de ce mot, blancs réduits — rustfmt peut le
/// couper sur plusieurs lignes sans changer l'étiquette.
fn scope_label(header: &str) -> Option<String> {
    let words: Vec<&str> = header.split_whitespace().collect();
    if let Some(at) = words.iter().rposition(|&w| w == "fn") {
        let name: String = words
            .get(at + 1)?
            .chars()
            .take_while(|&c| c.is_alphanumeric() || c == '_')
            .collect();
        return (!name.is_empty()).then_some(name);
    }
    let at = words
        .iter()
        .position(|&w| matches!(w, "mod" | "impl" | "trait") || w.starts_with("impl<"))?;
    Some(words[at..].join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un trou de l'étape `step` dans le dossier de travail, indenté de quatre espaces.
    fn hole(step: u8, body: &str) -> String {
        format!("    // >>> ÉTAPE {step} — à compléter\n    {body}\n    // <<< ÉTAPE {step}\n")
    }

    fn keys(text: &str) -> Vec<BlockKey> {
        block_keys(text, &find_blocks(text, REGION_BEGIN, REGION_END))
    }

    fn key(step: u8, scope: &str, rank: usize) -> BlockKey {
        (step, scope.to_string(), rank)
    }

    #[test]
    fn a_hole_is_keyed_by_its_enclosing_scope() {
        let todo = "todo!()";
        let text = format!(
            "pub(crate) const unsafe extern \"C\" fn first<T>(x: T)\nwhere\n    T: Fn(u32),\n{{\n{}}}\n\
             impl Mesh {{\n    #[cfg_attr(test, allow(dead_code))]\n    pub async fn second(&self) -> fn(u8) {{\n        \
             let s = \"}} {{\"; let c = '{{'; let r = r#\"}}\"#; /* }} */ // }}\n        if true {{ let f = |x: u8| {{\n\
             {}        }}; }}\n    }}\n}}\n{}",
            hole(1, todo),
            hole(1, todo),
            hole(1, "struct Unrelated;"),
        );
        assert_eq!(
            keys(&text),
            [
                key(1, "first", 0),
                key(1, "impl Mesh::second", 0),
                key(1, "", 0)
            ]
        );
    }

    #[test]
    fn a_hole_inserted_above_does_not_shift_the_others() {
        let old = format!("fn build_faces() {{\n{}}}\n", hole(2, "let mine = 1;"));
        let new = format!(
            "fn edge_key() {{\n{}}}\nfn build_faces() {{\n{}}}\n",
            hole(2, "todo!()"),
            hole(2, "todo!()")
        );
        assert_eq!(keys(&old)[0], keys(&new)[1]);

        let none = Reference::new();
        let (merged, changed, lost) = merge("src/lib.rs", &old, &new, &none, &none);
        assert_eq!((changed, lost.len()), (1, 0));
        let edge_key = merged.find("fn edge_key").unwrap();
        let build_faces = merged.find("fn build_faces").unwrap();
        let mine = merged.find("let mine = 1;").unwrap();
        assert!(edge_key < build_faces && build_faces < mine);
    }

    #[test]
    fn methods_of_the_same_name_are_told_apart_by_their_impl() {
        let old = format!(
            "impl<T> From<T>\n    for A\n{{\n    fn from(_: T) -> Self {{\n{}    }}\n}}\n",
            hole(3, "let mine = 1;")
        );
        let new = format!(
            "impl From<u8> for B {{\n    fn from(_: u8) -> Self {{\n{}    }}\n}}\n{old}",
            hole(3, "todo!()")
        );
        assert_eq!(keys(&old), [key(3, "impl<T> From<T> for A::from", 0)]);
        assert_eq!(keys(&old)[0], keys(&new)[1]);
    }

    #[test]
    fn a_half_written_answer_does_not_disturb_the_next_keys() {
        let text = format!(
            "fn first() {{\n{}}}\nfn second() {{\n{}}}\n",
            hole(3, "let s = \"; if x { {"),
            hole(3, "todo!()")
        );
        assert_eq!(keys(&text), [key(3, "first", 0), key(3, "second", 0)]);
    }

    #[test]
    fn a_renamed_function_loses_its_answer_loudly() {
        let old = format!("fn before() {{\n{}}}\n", hole(4, "let mine = 1;"));
        let new = format!("fn after() {{\n{}}}\n", hole(4, "todo!()"));
        let none = Reference::new();
        let (_, changed, lost) = merge("src/lib.rs", &old, &new, &none, &none);
        assert_eq!(changed, 0);
        assert_eq!(lost.len(), 1);
        assert!(lost[0].contains("dans before"), "{}", lost[0]);
    }
}
