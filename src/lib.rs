//! Outillage commun des fils rouges de formation (`cargo xtrain`).
//!
//! Le dépôt d'un fil rouge contient le code complet. Le stagiaire s'en fabrique un dossier
//! de travail, où les passages à écrire sont remplacés par des `todo!()` :
//!
//! ```shell
//! cargo xtrain start                  # crée travail/
//! cd travail
//! ```
//!
//! Chaque bloc encadré par `// SOLUTION-BEGIN` / `// SOLUTION-END` y devient un
//! `todo!()` entouré de marqueurs, et le texte d'origine part dans
//! `xtrain/reference.txt` — c'est de là que viennent les rattrapages.
//!
//! Depuis ce dossier de travail, il pilote ensuite son avancement :
//!
//! ```shell
//! cargo xtrain status                 # où j'en suis
//! cargo xtrain goto 3                 # passer à l'étape 3
//! cargo xtrain next                   # passer à l'étape suivante
//! cargo xtrain solve                  # remplir l'étape en cours pour moi (rattrapage)
//! cargo xtrain solve 2                # remplir l'étape 2
//! cargo xtrain reset 2                # la rouvrir pour la refaire
//! ```
//!
//! Deux garde-fous, parce que ces commandes sont lancées par quelqu'un qui découvre le
//! projet : `goto` ne remplit que les trous **restés vides**, et `start` refuse
//! d'écraser un dossier de travail existant sans `--force`. `start` écrit dans
//! `travail/` par défaut ; `--dir <chemin>` en choisit un autre.
//!
//! Les exercices courts (`exercices/<m>`) suivent le même principe : le dépôt en
//! contient les corrigés, annotés, dont `start` tire les énoncés (voir `exercises`).
//!
//! Pendant la mise au point des exercices, `cargo xtrain update [--dir <chemin>]`, lancé
//! depuis le corrigé, reporte ses modifications dans un dossier de travail existant sans
//! perdre les trous déjà remplis, les exercices courts modifiés, ni l'étape courante.
//!
//! # Brancher un fil rouge
//!
//! Chaque dépôt garde une crate binaire minuscule, `xtrain/`, membre de son workspace et
//! livrée dans le dossier de travail. Elle ne contient que la description du fil rouge
//! (un [`Project`]) et son point d'entrée :
//!
//! ```ignore
//! const PROJECT: xtask_training::Project = xtask_training::Project { /* … */ };
//!
//! fn main() {
//!     xtask_training::main(&PROJECT, env!("CARGO_MANIFEST_DIR"));
//! }
//!
//! #[test]
//! fn the_project_matches_the_repository() {
//!     xtask_training::check(&PROJECT, env!("CARGO_MANIFEST_DIR")).unwrap();
//! }
//! ```
//!
//! et l'alias de `.cargo/config.toml` :
//!
//! ```toml
//! [alias]
//! xtrain = "run --quiet --package xtrain --"
//! ```
//!
//! `env!("CARGO_MANIFEST_DIR")` doit être évalué dans la crate du dépôt, pas ici : c'est
//! lui qui désigne la racine du dépôt (le parent de `xtrain/`).
//!
//! Organisation : `cli` analyse la ligne de commande et aiguille vers `starter`
//! (`start`), `steps` (`status`, `goto`, `next`, `solve`, `reset`), `update` et
//! `verify` ; tous s'appuient sur `blocks` (les blocs à compléter), `exercises`
//! (les énoncés des exercices courts) et `files` (le parcours du dépôt). `project`
//! décrit ce qui change d'un fil rouge à l'autre.

mod blocks;
mod check;
mod cli;
mod exercises;
mod files;
mod inventory;
mod project;
mod starter;
mod steps;
mod update;
mod verify;

pub use crate::project::{Project, REPOSITORY, StepCheck, TOOL_DIR, check};

use crate::cli::{Command, Place, parse, run, usage};
use std::path::{Path, PathBuf};

/// Point d'entrée de `cargo xtrain` : analyse `std::env::args()` et lance la commande.
///
/// `manifest_dir` est le `env!("CARGO_MANIFEST_DIR")` de la crate `xtrain/` du dépôt ;
/// la racine du dépôt est son parent. Termine le processus en cas d'erreur (code 2 pour
/// une ligne de commande incomprise, 1 pour une commande en échec).
pub fn main(project: &Project, manifest_dir: &str) {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = repository_root(manifest_dir);
    let place = Place::of(&root);
    let command = match parse(project, place, &args) {
        Ok(Command::Help) => {
            print!("{}", usage(project, place));
            return;
        }
        Ok(command) => command,
        Err(e) => {
            eprintln!("erreur : {e}\n\n{}", usage(project, place));
            std::process::exit(2);
        }
    };

    if let Err(e) = run(project, command, &root) {
        eprintln!("erreur : {e}");
        std::process::exit(1);
    }
}

/// Racine du dépôt : le répertoire parent de celui du `Cargo.toml` de `xtrain/`.
pub(crate) fn repository_root(manifest_dir: &str) -> PathBuf {
    let manifest = PathBuf::from(manifest_dir);
    manifest.parent().map(Path::to_path_buf).unwrap_or(manifest)
}
