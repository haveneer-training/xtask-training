//! Blocs à compléter : repérage des marqueurs et réécriture des fichiers.

/// Marqueur ouvrant dans le dépôt corrigé.
pub const SOLUTION_BEGIN: &str = "// SOLUTION-BEGIN";

/// Marqueur fermant dans le dépôt corrigé.
pub const SOLUTION_END: &str = "// SOLUTION-END";

/// Préfixe du marqueur ouvrant dans le dépôt de travail.
pub const REGION_BEGIN: &str = "// >>> ÉTAPE ";

/// Préfixe du marqueur fermant dans le dépôt de travail.
pub const REGION_END: &str = "// <<< ÉTAPE ";

/// Ce que dit un marqueur ouvrant ordinaire, dans le dépôt de travail.
pub const FILL_NOTE: &str = "à compléter";

/// Ce que dit le marqueur ouvrant d'un trou **vide**, dans le dépôt de travail.
///
/// Un trou vide n'a pas de `todo!()` : le stagiaire y écrit des *définitions* — des
/// types, des `impl` — et non le corps d'une fonction qui existe déjà. `todo!()` est une
/// expression : il ne peut pas tenir lieu de `struct` ni d'`enum`, d'où cette seconde
/// forme de trou. Dans le corrigé, le marqueur ouvrant porte alors `vide`.
pub const BLANK_NOTE: &str = "à écrire de zéro";

/// Un bloc à compléter, repéré dans un fichier source.
pub struct Block {
    pub step: u8,
    /// Trou **vide** : remplacé par rien du tout, et non par un `todo!()`.
    pub blank: bool,
    /// Indentation à réutiliser pour le `todo!()`.
    pub indent: String,
    /// Lignes du corps, marqueurs exclus.
    pub body: Vec<String>,
    /// Numéro d'ordre du bloc dans son fichier.
    pub ordinal: usize,
    /// Ligne du marqueur ouvrant.
    pub start: usize,
    /// Ligne du marqueur fermant.
    pub end: usize,
}

/// Repère les blocs délimités par `begin` / `end` dans un fichier source.
///
/// L'étape est lue sur le commentaire `TODO-STEP:<n>` qui précède le bloc dans le
/// dépôt corrigé, ou sur le marqueur lui-même dans le dépôt de travail.
pub fn find_blocks(text: &str, begin: &str, end: &str) -> Vec<Block> {
    let lines: Vec<&str> = text.lines().collect();
    let mut blocks = Vec::new();
    let mut ordinal = 0;
    let mut i = 0;

    while i < lines.len() {
        if !lines[i].trim_start().starts_with(begin) {
            i += 1;
            continue;
        }
        let start = i;
        let indent: String = lines[i].chars().take_while(|c| c.is_whitespace()).collect();

        let step = step_of(&lines, start, begin).unwrap_or_else(|| {
            // Bloc sans `TODO-STEP:<n>` lisible juste au-dessus : erreur d'écriture du
            // corrigé. Mieux vaut s'arrêter que d'attribuer silencieusement l'étape 0.
            panic!(
                "bloc {begin} ligne {} sans commentaire TODO-STEP:<n> dans le bloc de \
                 commentaires qui le précède",
                start + 1
            )
        });
        let Some(stop) = (start + 1..lines.len()).find(|&k| lines[k].trim_start().starts_with(end))
        else {
            break;
        };

        blocks.push(Block {
            step,
            blank: is_blank_marker(lines[start]),
            indent,
            body: lines[start + 1..stop]
                .iter()
                .map(|l| l.to_string())
                .collect(),
            ordinal,
            start,
            end: stop,
        });
        ordinal += 1;
        i = stop + 1;
    }
    blocks
}

/// Le marqueur ouvrant annonce-t-il un trou vide ?
///
/// `// SOLUTION-BEGIN vide` dans le corrigé, la mention correspondante dans le dépôt de
/// travail — les deux dépôts sont relus par le même code.
fn is_blank_marker(line: &str) -> bool {
    line.trim_end().ends_with(" vide") || line.contains(BLANK_NOTE)
}

/// Numéro d'étape associé à un bloc.
fn step_of(lines: &[&str], start: usize, begin: &str) -> Option<u8> {
    // dépôt de travail : le numéro est sur le marqueur lui-même
    if begin == REGION_BEGIN {
        return lines[start]
            .trim_start()
            .strip_prefix(REGION_BEGIN)?
            .split_whitespace()
            .next()?
            .parse()
            .ok();
    }
    // dépôt corrigé : on remonte le bloc de commentaires qui précède le marqueur,
    // jusqu'au TODO-STEP le plus proche. Pas de limite en nombre de lignes : le bloc
    // s'arrête de lui-même à la première ligne qui n'est pas un commentaire.
    (0..start)
        .rev()
        .take_while(|&k| lines[k].trim_start().starts_with("//"))
        .find_map(|k| {
            let (_, rest) = lines[k].split_once("TODO-STEP:")?;
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            digits.parse().ok()
        })
}

/// Reconstruit un fichier en remplaçant le corps de certains blocs.
///
/// `replacement` reçoit un bloc et renvoie les lignes à mettre à sa place, ou `None`
/// pour le laisser tel quel.
pub fn rewrite(
    text: &str,
    blocks: &[Block],
    mut replacement: impl FnMut(&Block) -> Option<Vec<String>>,
) -> (String, usize) {
    let lines: Vec<&str> = text.lines().collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut changed = 0;
    let mut i = 0;

    while i < lines.len() {
        match blocks.iter().find(|b| b.start == i) {
            Some(block) => match replacement(block) {
                Some(body) => {
                    out.push(lines[block.start].to_string());
                    out.extend(body);
                    out.push(lines[block.end].to_string());
                    changed += 1;
                    i = block.end + 1;
                }
                None => {
                    out.push(lines[i].to_string());
                    i += 1;
                }
            },
            None => {
                out.push(lines[i].to_string());
                i += 1;
            }
        }
    }
    (out.join("\n") + "\n", changed)
}

/// La ligne `todo!()` qui tient lieu de travail à faire.
fn todo_line(indent: &str, step: u8) -> String {
    format!("{indent}todo!(\"étape {step} — voir le commentaire ci-dessus\")")
}

/// Ce qui tient lieu de travail à faire dans le dépôt de travail : un `todo!()`, ou
/// rien du tout pour un trou vide.
pub fn hole_body(block: &Block) -> Vec<String> {
    if block.blank {
        Vec::new()
    } else {
        vec![todo_line(&block.indent, block.step)]
    }
}

/// Un bloc est « vide » tant qu'il contient encore son `todo!()` — ou, pour un trou
/// vide, tant que le stagiaire n'y a rien écrit du tout.
pub fn is_empty(block: &Block) -> bool {
    block.body.iter().any(|l| l.contains("todo!("))
        || block.body.iter().all(|l| l.trim().is_empty())
}
