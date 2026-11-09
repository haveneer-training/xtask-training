//! Exercices courts : le fichier du dépôt est le corrigé, l'énoncé en est tiré.
//!
//! Comme pour le fil rouge, `exercices/<m>/examples/` contient les **corrigés**, annotés ;
//! `cargo xtrain start` en tire les énoncés du dossier de travail. Dans un corrigé :
//!
//! ```text
//!     // SOLUTION-BEGIN
//!     failures.push(ts);
//!     // SOLUTION-ELSE
//!     // todo!("ajouter à la fin")
//!     // SOLUTION-END
//! ```
//!
//! - Les lignes entre `SOLUTION-BEGIN` et `SOLUTION-ELSE` (ou `SOLUTION-END`) ne valent
//!   que pour le corrigé : l'énoncé ne les a pas.
//! - Les lignes entre `SOLUTION-ELSE` et `SOLUTION-END`, commentées pour que le corrigé
//!   compile, sont celles de l'énoncé : on y retire le premier `// ` (une ligne `//` seule
//!   devient vide). C'est un `todo!()`, ou, au module 1, le code fautif à corriger.
//! - Sans `SOLUTION-ELSE`, le bloc disparaît de l'énoncé : une section « Corrigé » de
//!   l'en-tête, une variante, une fonction que le stagiaire doit écrire de zéro…
//! - La mention « (corrigé) » qui termine la première ligne (le titre) est retirée.
//!
//! Les blocs ne s'imbriquent pas. Contrairement aux trous du fil rouge, ils ne servent pas
//! à `goto` ni à `solve` : l'énoncé ne garde aucun marqueur, et le corrigé reste à
//! consulter dans le dépôt.
//!
//! Une ligne commentée qui suit une signature de fonction est réindentée par `rustfmt`
//! comme le corps : un bloc couvre donc la fonction entière plutôt que sa seule signature.
//! `cargo xtrain verify` vérifie que chaque énoncé tiré est bien formaté.
//!
//! Le dossier de travail garde une copie des énoncés livrés, sous [`DELIVERED`] :
//! `cargo xtrain update` y voit si le stagiaire a touché à un exercice (voir
//! [`carry_over`]).

use crate::files::{relative, rust_files_in};
use crate::project::Project;
use std::fs;
use std::path::{Path, PathBuf};

const BEGIN: &str = "// SOLUTION-BEGIN";
const ELSE: &str = "// SOLUTION-ELSE";
const END: &str = "// SOLUTION-END";

/// Où le dossier de travail garde les énoncés livrés, sous leur chemin d'origine.
pub const DELIVERED: &str = "xtrain/enonces";

/// Suffixe du nouvel énoncé déposé à côté d'un exercice que le stagiaire a modifié.
const NEW_SUFFIX: &str = ".new";

/// Ce qui termine le titre d'un corrigé.
const TITLE_MARK: &str = " (corrigé)";

/// Les exercices courts de `module`, sous `root`.
pub fn exercise_files(root: &Path, module: &str) -> Vec<PathBuf> {
    rust_files_in(&root.join("exercices").join(module).join("examples"))
}

/// Les exercices courts de tous les modules, sous `root`, avec leur chemin relatif.
fn all_exercises(p: &Project, root: &Path) -> Vec<(String, PathBuf)> {
    p.short_exercises
        .iter()
        .flat_map(|module| exercise_files(root, module))
        .map(|path| (relative(root, &path), path))
        .collect()
}

/// Remplace, sous `out`, chaque corrigé d'exercice court par son énoncé, en garde une
/// copie sous [`DELIVERED`], et renvoie le nombre d'exercices.
pub fn write_statements(p: &Project, out: &Path) -> Result<usize, String> {
    let exercises = all_exercises(p, out);
    for (key, path) in &exercises {
        let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
        let statement = statement(&text).map_err(|e| format!("{key} : {e}"))?;
        fs::write(path, &statement).map_err(|e| e.to_string())?;
        write(&out.join(DELIVERED).join(key), &statement)?;
    }
    Ok(exercises.len())
}

/// Ce que [`carry_over`] a fait, ou n'a pas pu faire.
#[derive(Debug, Default, PartialEq)]
pub struct CarryOver {
    /// Exercices modifiés par le stagiaire, conservés : une ligne par exercice.
    pub kept: Vec<String>,
    /// Exercices modifiés qui disparaîtraient : la mise à jour doit s'arrêter.
    pub lost: Vec<String>,
}

/// Replace dans `staging`, le dossier de travail fraîchement engendré, les exercices
/// courts sur lesquels le stagiaire a travaillé dans `out`.
///
/// Un exercice est « modifié » s'il diffère de l'énoncé livré (copie sous
/// [`DELIVERED`] ; sans copie, dossier de travail plus ancien, s'il diffère du nouvel
/// énoncé). Alors :
///
/// - si l'énoncé n'a pas changé dans le corrigé, l'exercice est conservé ;
/// - s'il a changé, l'exercice est conservé, et le nouvel énoncé déposé à côté, en
///   `<nom>.rs.new` (que Cargo ignore), à fusionner à la main ;
/// - s'il a disparu du corrigé, il serait perdu : signalé dans [`CarryOver::lost`].
///
/// Un exercice non modifié prend le nouvel énoncé, ou disparaît avec lui. Un fichier
/// ajouté par le stagiaire dans `examples/`, et les `.new` d'une mise à jour
/// précédente, sont conservés.
pub fn carry_over(p: &Project, out: &Path, staging: &Path) -> Result<CarryOver, String> {
    let mut result = CarryOver::default();
    let read = |path: &Path| fs::read_to_string(path).ok();
    let delivered_dir = out.join(DELIVERED);
    let fresh = all_exercises(p, staging);

    for (key, path) in &fresh {
        let Some(current) = read(&out.join(key)) else {
            continue;
        };
        let new = read(path).ok_or_else(|| format!("{key} illisible"))?;
        let delivered = read(&delivered_dir.join(key));
        if current == new || delivered.as_ref() == Some(&current) {
            continue;
        }
        fs::write(path, &current).map_err(|e| e.to_string())?;
        let note = match delivered {
            Some(d) if d == new => String::new(),
            // dossier de travail sans copie des énoncés livrés : impossible de savoir si
            // l'énoncé a changé
            None => format!(" ; l'énoncé actuel est dans {key}{NEW_SUFFIX}, pour comparer"),
            Some(_) => format!(" ; l'énoncé a changé, le nouveau est dans {key}{NEW_SUFFIX}"),
        };
        if !note.is_empty() {
            write(&staging.join(format!("{key}{NEW_SUFFIX}")), &new)?;
        }
        result.kept.push(format!("{key} : conservé{note}"));
    }

    for (key, path) in all_exercises(p, out) {
        if fresh.iter().any(|(k, _)| *k == key) {
            continue;
        }
        let current = read(&path).ok_or_else(|| format!("{key} illisible"))?;
        match read(&delivered_dir.join(&key)) {
            Some(delivered) if delivered == current => {}
            Some(_) => result.lost.push(format!(
                "{key} : cet exercice n'existe plus dans le corrigé"
            )),
            None => {
                write(&staging.join(&key), &current)?;
                result
                    .kept
                    .push(format!("{key} : ajouté par vous, conservé"));
            }
        }
    }

    // Les `.new` pas encore fusionnés, tant que leur exercice existe.
    for module in p.short_exercises {
        let dir = Path::new("exercices").join(module).join("examples");
        let Ok(entries) = fs::read_dir(out.join(&dir)) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(exercise) = name.strip_suffix(NEW_SUFFIX) else {
                continue;
            };
            let target = staging.join(&dir).join(&name);
            if staging.join(&dir).join(exercise).is_file() && !target.exists() {
                fs::copy(entry.path(), &target).map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(result)
}

/// Écrit un fichier, en créant ses répertoires au besoin.
fn write(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(path, text).map_err(|e| format!("{} : {e}", path.display()))
}

/// Où l'on en est d'un bloc, ligne après ligne.
#[derive(Clone, Copy, PartialEq)]
enum Part {
    Outside,
    Solution,
    Statement,
}

/// L'énoncé tiré d'un corrigé (voir l'en-tête du module).
pub fn statement(text: &str) -> Result<String, String> {
    let mut out: Vec<String> = Vec::new();
    let mut part = Part::Outside;
    let mut opened = 0;
    // Un bloc qui ne laisse rien dans l'énoncé, entre deux lignes vides : la seconde est
    // de trop.
    let mut skip_blank = false;

    for (n, line) in text.lines().enumerate() {
        let error = |what: &str| format!("ligne {} : {what}", n + 1);
        match (line.trim(), part) {
            (BEGIN, Part::Outside) => {
                part = Part::Solution;
                opened = out.len();
            }
            (BEGIN, _) => return Err(error("SOLUTION-BEGIN dans un bloc déjà ouvert")),
            (ELSE, Part::Solution) => part = Part::Statement,
            (ELSE, _) => return Err(error("SOLUTION-ELSE hors d'un bloc")),
            (END, Part::Outside) => return Err(error("SOLUTION-END sans SOLUTION-BEGIN")),
            (END, _) => {
                part = Part::Outside;
                skip_blank = out.len() == opened && out.last().is_some_and(String::is_empty);
            }
            (_, Part::Solution) => {}
            (_, Part::Statement) => out.push(uncomment(line).ok_or_else(|| {
                error("ligne d'énoncé non commentée (`// …` attendu après SOLUTION-ELSE)")
            })?),
            (_, Part::Outside) => {
                if !std::mem::take(&mut skip_blank) || !line.is_empty() {
                    out.push(line.to_string());
                }
            }
        }
    }
    if part != Part::Outside {
        return Err("SOLUTION-BEGIN sans SOLUTION-END".to_string());
    }
    if let Some(title) = out.first_mut()
        && let Some(stripped) = title.strip_suffix(TITLE_MARK)
    {
        *title = stripped.to_string();
    }
    Ok(out.join("\n") + "\n")
}

/// Une ligne d'énoncé, commentée dans le corrigé : `    // texte` → `    texte`,
/// `    //` → ligne vide. `None` si elle n'est pas commentée.
fn uncomment(line: &str) -> Option<String> {
    let indent = &line[..line.len() - line.trim_start().len()];
    let rest = line.trim_start();
    if rest == "//" {
        return Some(String::new());
    }
    rest.strip_prefix("// ")
        .map(|code| format!("{indent}{code}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statement_keeps_the_else_part() {
        let solution = "\
//! Titre (corrigé)
fn f() -> u32 {
    // SOLUTION-BEGIN
    1
    // SOLUTION-ELSE
    // todo!(\"un\")
    // SOLUTION-END
}
";
        assert_eq!(
            statement(solution).unwrap(),
            "//! Titre\nfn f() -> u32 {\n    todo!(\"un\")\n}\n"
        );
    }

    #[test]
    fn statement_keeps_relative_indentation_and_empty_lines() {
        let solution = "\
fn main() {
    // SOLUTION-BEGIN
    // SOLUTION-ELSE
    // for x in v {
    //     f(x);
    //
    //     // un commentaire
    // }
    // SOLUTION-END
}
";
        assert_eq!(
            statement(solution).unwrap(),
            "fn main() {\n    for x in v {\n        f(x);\n\n        // un commentaire\n    }\n}\n"
        );
    }

    #[test]
    fn a_block_without_else_vanishes_with_one_of_its_blank_lines() {
        let solution = "\
struct A;

// SOLUTION-BEGIN
struct B;
// SOLUTION-END

fn main() {}
";
        assert_eq!(statement(solution).unwrap(), "struct A;\n\nfn main() {}\n");
        // un bloc d'en-tête, sans ligne vide autour
        let solution = "//! a\n// SOLUTION-BEGIN\n//!\n//! b\n// SOLUTION-END\n\nfn main() {}\n";
        assert_eq!(statement(solution).unwrap(), "//! a\n\nfn main() {}\n");
    }

    /// Un dossier vide, propre à un test.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("xtrain-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn carry_over_keeps_what_the_trainee_wrote() {
        let p = &crate::project::TEST_PROJECT;
        let examples = format!("exercices/{}/examples", p.short_exercises[0]);
        let root = scratch("carry-over");
        let (out, staging) = (root.join("out"), root.join("staging"));
        let file = |dir: &Path, name: &str, text: &str| write(&dir.join(name), text).unwrap();
        let delivered =
            |name: &str, text: &str| file(&out, &format!("{DELIVERED}/{examples}/{name}"), text);
        let ex = |name: &str| format!("{examples}/{name}");

        // a : non touché, énoncé changé → nouvel énoncé
        delivered("a.rs", "a1");
        file(&out, &ex("a.rs"), "a1");
        file(&staging, &ex("a.rs"), "a2");
        // b : modifié, énoncé inchangé → conservé
        delivered("b.rs", "b1");
        file(&out, &ex("b.rs"), "b mine");
        file(&staging, &ex("b.rs"), "b1");
        // c : modifié, énoncé changé → conservé, nouvel énoncé à côté
        delivered("c.rs", "c1");
        file(&out, &ex("c.rs"), "c mine");
        file(&staging, &ex("c.rs"), "c2");
        // d : ajouté par le stagiaire → conservé
        file(&out, &ex("d.rs"), "d mine");
        // e : retiré du corrigé, non touché → disparaît
        delivered("e.rs", "e1");
        file(&out, &ex("e.rs"), "e1");
        // f : retiré du corrigé, modifié → perdu
        delivered("f.rs", "f1");
        file(&out, &ex("f.rs"), "f mine");
        // g : `.new` d'une mise à jour précédente → conservé
        delivered("g.rs", "g2");
        file(&out, &ex("g.rs"), "g mine");
        file(&out, &ex("g.rs.new"), "g2");
        file(&staging, &ex("g.rs"), "g2");

        let result = carry_over(p, &out, &staging).unwrap();
        let read = |name: &str| fs::read_to_string(staging.join(ex(name))).ok();
        assert_eq!(read("a.rs").as_deref(), Some("a2"));
        assert_eq!(read("a.rs.new"), None);
        assert_eq!(read("b.rs").as_deref(), Some("b mine"));
        assert_eq!(read("b.rs.new"), None);
        assert_eq!(read("c.rs").as_deref(), Some("c mine"));
        assert_eq!(read("c.rs.new").as_deref(), Some("c2"));
        assert_eq!(read("d.rs").as_deref(), Some("d mine"));
        assert_eq!(read("e.rs"), None);
        assert_eq!(read("g.rs").as_deref(), Some("g mine"));
        assert_eq!(read("g.rs.new").as_deref(), Some("g2"));
        assert_eq!(result.kept.len(), 4, "{:?}", result.kept);
        assert_eq!(result.lost.len(), 1, "{:?}", result.lost);
        assert!(result.lost[0].starts_with(&ex("f.rs")));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn statement_rejects_malformed_blocks() {
        for solution in [
            "// SOLUTION-BEGIN\n",
            "// SOLUTION-END\n",
            "// SOLUTION-ELSE\n",
            "// SOLUTION-BEGIN\n// SOLUTION-BEGIN\n// SOLUTION-END\n",
            "// SOLUTION-BEGIN\n// SOLUTION-ELSE\nfn f() {}\n// SOLUTION-END\n",
        ] {
            assert!(
                statement(solution).is_err(),
                "{solution:?} devrait être refusé"
            );
        }
    }
}
