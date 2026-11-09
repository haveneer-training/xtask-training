//! Commandes du dossier de travail : avancement, passage d'étape, rattrapage.

use crate::blocks::{
    REGION_BEGIN, REGION_END, SOLUTION_BEGIN, find_blocks, hole_body, is_empty, rewrite,
};
use crate::files::{relative, source_files};
use crate::project::Project;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// Où sont rangés les blocs de référence, dans le dépôt de travail (sous
/// [`crate::TOOL_DIR`]).
pub const REFERENCE: &str = "xtrain/reference.txt";

/// Charge les blocs de référence : `(chemin relatif, numéro d'ordre) → corps`.
pub fn load_reference(root: &Path) -> Result<BTreeMap<(String, usize), Vec<String>>, String> {
    let path = root.join(REFERENCE);
    let text = fs::read_to_string(&path).map_err(|e| {
        format!(
            "{} illisible ({e}).\nCette commande s'utilise dans le dépôt de travail, \
             pas dans le corrigé.",
            path.display()
        )
    })?;

    let mut map = BTreeMap::new();
    let mut key: Option<(String, usize)> = None;
    let mut body: Vec<String> = Vec::new();

    for line in text.lines() {
        if let Some(header) = line.strip_prefix("@@ ") {
            let fields: Vec<&str> = header.split(" @@ ").collect();
            if fields.len() == 3 {
                key = Some((
                    fields[0].to_string(),
                    fields[2].parse().map_err(|_| "numéro d'ordre illisible")?,
                ));
                body.clear();
            }
            continue;
        }
        if line == "@@" {
            if let Some(k) = key.take() {
                map.insert(k, std::mem::take(&mut body));
            }
            continue;
        }
        body.push(line.to_string());
    }
    Ok(map)
}

/// Remplit les trous d'une étape ; `force` écrase même ce que le stagiaire a écrit.
pub fn fill(p: &Project, root: &Path, step: u8, force: bool) -> Result<(), String> {
    let reference = load_reference(root)?;
    let files = source_files(p, root);

    let mut total = 0;
    for path in files {
        let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let blocks = find_blocks(&text, REGION_BEGIN, REGION_END);
        let key = relative(root, &path);

        let (new_text, changed) = rewrite(&text, &blocks, |b| {
            if b.step != step || (!force && !is_empty(b)) {
                return None;
            }
            reference.get(&(key.clone(), b.ordinal)).cloned()
        });
        if changed > 0 {
            fs::write(&path, new_text).map_err(|e| e.to_string())?;
            println!("  {key} : {changed} bloc(s) rempli(s)");
            total += changed;
        }
    }
    if total == 0 {
        println!("  étape {step} : rien à remplir");
    }
    Ok(())
}

/// Rouvre les trous d'une étape.
pub fn reset(p: &Project, root: &Path, step: u8) -> Result<(), String> {
    let files = source_files(p, root);

    for path in files {
        let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let blocks = find_blocks(&text, REGION_BEGIN, REGION_END);
        let (new_text, changed) = rewrite(&text, &blocks, |b| {
            (b.step == step && !is_empty(b)).then(|| hole_body(b))
        });
        if changed > 0 {
            fs::write(&path, new_text).map_err(|e| e.to_string())?;
            println!("  {} : {changed} bloc(s) rouvert(s)", relative(root, &path));
        }
    }
    Ok(())
}

/// Passe le projet à l'étape demandée.
pub fn goto(p: &Project, root: &Path, step: u8) -> Result<(), String> {
    println!("Passage à l'étape {step}.");
    for earlier in 0..step {
        fill(p, root, earlier, false)?;
    }
    set_default_feature(p, root, step)?;
    println!(
        "Les tests des étapes 0 à {step} sont actifs. Lancez `cargo test`, \
         puis complétez les `todo!()` de l'étape {step}."
    );
    Ok(())
}

/// Va au bout du parcours : la dernière étape, avec les trous **restés vides** de toutes
/// les étapes remplis — la dernière comprise, contrairement à `goto <dernière>`. Comme
/// `goto`, n'écrase jamais ce que le stagiaire a écrit (`solve`, lui, l'écrase).
pub fn goto_end(p: &Project, root: &Path) -> Result<(), String> {
    let last = p.last_step;
    println!("Passage à la fin du parcours (étape {last}).");
    for step in 0..=last {
        fill(p, root, step, false)?;
    }
    set_default_feature(p, root, last)?;
    println!(
        "Les tests des étapes 0 à {last} sont actifs, tous les trous restés vides sont \
         remplis : `cargo test` doit être vert."
    );
    Ok(())
}

/// Passe à l'étape qui suit l'étape courante.
pub fn next(p: &Project, root: &Path) -> Result<(), String> {
    let step = required_current_step(p, root)?;
    let last = p.last_step;
    match next_step(p, step) {
        Some(n) => goto(p, root, n),
        None if step == last => Err(format!(
            "vous êtes déjà à la dernière étape ({last}) : il n'y a pas d'étape suivante"
        )),
        None => Err("ce dépôt est le corrigé : il n'y a pas d'étape à ouvrir.\n\
             Engendrez le dépôt de travail avec `cargo xtrain start`."
            .to_string()),
    }
}

/// L'étape qui suit `step`, s'il y en a une. Au-delà de [`Project::last_step`] (la
/// sentinelle du corrigé), il n'y en a pas.
/// `solve` sans numéro : remplit les trous de l'étape en cours.
pub fn solve_current(p: &Project, root: &Path) -> Result<(), String> {
    let step = required_current_step(p, root)?;
    if step > p.last_step {
        return Err(
            "ce dépôt est le corrigé : il n'y a pas d'étape à remplir.\n\
             Engendrez le dépôt de travail avec `cargo xtrain start`."
                .to_string(),
        );
    }
    fill(p, root, step, true)
}

fn required_current_step(p: &Project, root: &Path) -> Result<u8, String> {
    current_step(p, root).ok_or_else(|| {
        format!(
            "étape courante introuvable (ligne `default = [...]` de {})",
            p.step_manifest
        )
    })
}

fn next_step(p: &Project, step: u8) -> Option<u8> {
    (step < p.last_step).then_some(step + 1)
}

/// Réécrit la ligne `default = [...]` du manifeste des étapes.
pub fn set_default_feature(p: &Project, root: &Path, step: u8) -> Result<(), String> {
    rewrite_default(&root.join(p.step_manifest), &format!("[\"step{step}\"]"))
}

/// Remplace la valeur de la ligne `default = [...]` d'un manifeste.
pub fn rewrite_default(path: &Path, value: &str) -> Result<(), String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut replaced = false;
    let new_text: Vec<String> = text
        .lines()
        .map(|line| {
            if line.starts_with("default = [") && !replaced {
                replaced = true;
                format!("default = {value}")
            } else {
                line.to_string()
            }
        })
        .collect();
    if !replaced {
        return Err(format!(
            "ligne `default = [...]` introuvable dans {}",
            path.display()
        ));
    }
    fs::write(path, new_text.join("\n") + "\n").map_err(|e| e.to_string())
}

/// Étape actuellement déclarée dans le manifeste.
pub fn current_step(p: &Project, root: &Path) -> Option<u8> {
    let text = fs::read_to_string(root.join(p.step_manifest)).ok()?;
    let line = text.lines().find(|l| l.starts_with("default = ["))?;
    let (_, rest) = line.split_once("step")?;
    rest.chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .ok()
}

/// Affiche l'avancement, étape par étape.
pub fn status(p: &Project, root: &Path) -> Result<(), String> {
    let files = source_files(p, root);

    let mut done: BTreeMap<u8, (usize, usize)> = BTreeMap::new();
    let mut corrected = false;
    for path in &files {
        let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
        if text.contains(SOLUTION_BEGIN) {
            corrected = true;
        }
        for block in find_blocks(&text, REGION_BEGIN, REGION_END) {
            let entry = done.entry(block.step).or_insert((0, 0));
            entry.1 += 1;
            if !is_empty(&block) {
                entry.0 += 1;
            }
        }
    }

    if corrected {
        println!("Ce dépôt est le corrigé : il n'y a rien à compléter.");
        println!("Engendrez le dépôt de travail avec `cargo xtrain start`.");
        return Ok(());
    }

    let step = current_step(p, root).unwrap_or(0);
    println!("Étape courante : {step}\n");
    for n in 0..=p.last_step {
        let (filled, total) = done.get(&n).copied().unwrap_or((0, 0));
        let mark = match (total, filled == total) {
            (0, _) => "  ",
            (_, true) => "OK",
            _ => "→ ",
        };
        let active = if n <= step { "tests actifs" } else { "" };
        println!("  {mark} étape {n} : {filled}/{total} bloc(s) complété(s)  {active}");
    }
    println!(
        "\n`cargo xtrain next` pour passer à la suivante, `cargo xtrain goto <n>` pour changer d'étape."
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::TEST_PROJECT;

    #[test]
    fn next_step_stops_at_the_last_one() {
        let p = &TEST_PROJECT;
        let last = p.last_step;
        assert_eq!(next_step(p, 0), Some(1));
        assert_eq!(next_step(p, last - 1), Some(last));
        assert_eq!(next_step(p, last), None);
        // la sentinelle du corrigé
        assert_eq!(next_step(p, last + 1), None);
    }
}
