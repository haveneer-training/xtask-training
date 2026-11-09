//! Ligne de commande : analyse d'argv et aiguillage vers les commandes.
//!
//! Analyse à la main, volontairement : l'outillage n'a aucune dépendance externe, et
//! `cargo xtrain start` est la toute première commande du stagiaire — elle doit marcher
//! hors ligne, sans attendre la compilation d'une crate comme `clap`, et répondre en
//! français. Ajouter une commande : une variante de [`Command`], un bras dans [`parse`],
//! un dans [`run`], une ligne dans [`usage`] — et, si elle ne sert que dans le corrigé, son
//! nom dans [`REFERENCE_ONLY`].
//!
//! Dans un dossier de travail, l'aide ne montre que les commandes du stagiaire, et les
//! commandes de gestion du corrigé y sont refusées avec une explication ([`Place`]).

use crate::check::{CheckOptions, check_steps};
use crate::inventory::inventory;
use crate::project::Project;
use crate::starter::{WORKDIR, make_starter};
use crate::steps::{REFERENCE, fill, goto, goto_end, next, reset, status};
use crate::update::update;
use crate::verify::verify;
use std::path::{Path, PathBuf};

/// Une commande de `cargo xtrain`, une fois la ligne de commande analysée.
#[derive(Debug, PartialEq)]
pub enum Command {
    /// `start [--dir <p>] [--force]` ; sans `--dir`, `{WORKDIR}/`.
    Start {
        dir: Option<PathBuf>,
        force: bool,
    },
    /// `update [--dir <p>]` ; sans `--dir`, `{WORKDIR}/`.
    Update {
        dir: Option<PathBuf>,
    },
    Status,
    Goto(u8),
    /// `goto end` : toutes les étapes, trous restés vides remplis.
    GotoEnd,
    Next,
    Solve(u8),
    Reset(u8),
    Verify,
    /// `check-steps [--keep] [--quick] [--step <n>|<a>..<b>]…`.
    CheckSteps(CheckOptions),
    Inventory,
    Help,
}

/// Exécute une commande depuis le dépôt `root`.
pub fn run(p: &Project, command: Command, root: &Path) -> Result<(), String> {
    let workdir = |dir: Option<PathBuf>| dir.unwrap_or_else(|| root.join(WORKDIR));
    match command {
        Command::Start { dir, force } => make_starter(p, root, &workdir(dir), force),
        Command::Update { dir } => update(p, root, &workdir(dir)),
        Command::Status => status(p, root),
        Command::Goto(n) => goto(p, root, n),
        Command::GotoEnd => goto_end(p, root),
        Command::Next => next(p, root),
        Command::Solve(n) => fill(p, root, n, true),
        Command::Reset(n) => reset(p, root, n),
        Command::Verify => verify(p, root),
        Command::CheckSteps(options) => check_steps(p, root, &options),
        Command::Inventory => inventory(p, root),
        Command::Help => Ok(()),
    }
}

/// Où `cargo xtrain` est lancé.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Place {
    /// Le dépôt du corrigé : toutes les commandes.
    Reference,
    /// Un dossier de travail (il a ses blocs de référence) : celles du stagiaire.
    Workdir,
}

impl Place {
    pub fn of(root: &Path) -> Place {
        if root.join(REFERENCE).is_file() {
            Place::Workdir
        } else {
            Place::Reference
        }
    }
}

/// Les commandes du stagiaire qui modifient le dossier : dans le corrigé, elles en
/// réécriraient le manifeste (`goto`) ou n'auraient pas de blocs de référence.
pub const WORKDIR_ONLY: [&str; 4] = ["goto", "next", "solve", "reset"];

/// Les commandes de gestion, qui ne fonctionnent que depuis le dépôt du corrigé.
pub const REFERENCE_ONLY: [&str; 5] = ["start", "update", "verify", "check-steps", "inventory"];

/// Analyse la ligne de commande, nom de la commande compris.
///
/// Tout argument non reconnu est une erreur : un drapeau mal tapé (`--frce`) ne doit pas
/// être ignoré en silence.
pub fn parse(p: &Project, place: Place, args: &[String]) -> Result<Command, String> {
    let Some((name, rest)) = args.split_first() else {
        return Ok(Command::Help);
    };
    if place == Place::Reference && WORKDIR_ONLY.contains(&name.as_str()) {
        return Err(format!(
            "`{name}` s'utilise dans un dossier de travail : dans le dépôt du corrigé, il en \
             modifierait le manifeste ou les sources. Engendrez-le avec `cargo xtrain start`, \
             puis `cd {WORKDIR}`"
        ));
    }
    if place == Place::Workdir && REFERENCE_ONLY.contains(&name.as_str()) {
        return Err(format!(
            "`{name}` sert à préparer les exercices, depuis le dépôt du corrigé : il ne \
             fonctionne pas dans un dossier de travail"
        ));
    }
    let unexpected = |arg: &str| format!("argument inattendu « {arg} » pour `{name}`");
    let no_args = |command: Command| match rest.first() {
        Some(arg) => Err(unexpected(arg)),
        None => Ok(command),
    };
    let step = || match rest {
        [] => Err("il manque le numéro d'étape".to_string()),
        [n] => parse_step(p, n),
        [_, arg, ..] => Err(unexpected(arg)),
    };

    match name.as_str() {
        "help" | "-h" | "--help" => no_args(Command::Help),
        "status" => no_args(Command::Status),
        "next" => no_args(Command::Next),
        "verify" => no_args(Command::Verify),
        "inventory" => no_args(Command::Inventory),
        "check-steps" => {
            let mut options = CheckOptions::default();
            let mut args = rest.iter();
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--keep" if !options.keep => options.keep = true,
                    "--quick" if !options.quick => options.quick = true,
                    "--step" => match args.next() {
                        Some(value) if !value.starts_with("--") => {
                            options.steps.extend(parse_steps(p, value)?)
                        }
                        _ => return Err("--step attend un numéro d'étape, ou a..b".to_string()),
                    },
                    _ => return Err(unexpected(arg)),
                }
            }
            options.steps.sort_unstable();
            options.steps.dedup();
            Ok(Command::CheckSteps(options))
        }
        "goto" if rest.first().is_some_and(|a| a == "end") => match rest {
            [_] => Ok(Command::GotoEnd),
            [_, arg, ..] => Err(unexpected(arg)),
            [] => unreachable!(),
        },
        "goto" => step().map(Command::Goto),
        "solve" => step().map(Command::Solve),
        "reset" => step().map(Command::Reset),
        "start" | "update" => {
            let mut dir = None;
            let mut force = false;
            let mut args = rest.iter();
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--dir" if dir.is_some() => return Err("--dir donné deux fois".to_string()),
                    "--dir" => match args.next() {
                        Some(value) if !value.starts_with("--") => dir = Some(PathBuf::from(value)),
                        _ => return Err("--dir attend un chemin".to_string()),
                    },
                    "--force" if name == "start" => force = true,
                    _ => return Err(unexpected(arg)),
                }
            }
            Ok(if name == "start" {
                Command::Start { dir, force }
            } else {
                Command::Update { dir }
            })
        }
        _ => Err(format!("commande inconnue « {name} »")),
    }
}

pub fn usage(p: &Project, place: Place) -> String {
    let (name, last) = (p.name, p.last_step);
    let trainee = format!(
        "  cargo xtrain status                où en est le projet, étape par étape\n\
         \x20 cargo xtrain goto <n>              passer à l'étape n (0 à {last})\n\
         \x20 cargo xtrain goto end              aller au bout : remplir tous les trous restés vides\n\
         \x20 cargo xtrain next                  passer à l'étape suivante\n\
         \x20 cargo xtrain solve <n>             remplir les trous de l'étape n\n\
         \x20 cargo xtrain reset <n>             rouvrir les trous de l'étape n\n"
    );
    let management = format!(
        "  cargo xtrain start [--dir <p>]     créer le dossier de travail \
(par défaut : {WORKDIR}/)\n\
         \x20 cargo xtrain update [--dir <p>]    y reporter les modifications du corrigé\n\
         \x20 cargo xtrain verify                vérifier les exercices courts\n\
         \x20 cargo xtrain check-steps           rejouer les étapes dans un dossier jetable\n\
         \x20     [--quick] [--step <n>|<a>..<b>] [--keep]   (contrôle rapide, étapes choisies)\n\
         \x20 cargo xtrain inventory             inventaire des trous, par étape (Markdown)\n"
    );
    match place {
        Place::Workdir => format!("Outillage du fil rouge « {name} »\n\n{trainee}"),
        Place::Reference => format!(
            "Outillage du fil rouge « {name} » — dépôt du corrigé\n\
             \n\
             Dans le dossier de travail :\n\
             {trainee}\
             \n\
             Dans le dépôt du corrigé :\n\
             {management}"
        ),
    }
}

/// `n` ou `a..b` (bornes comprises).
fn parse_steps(p: &Project, raw: &str) -> Result<Vec<u8>, String> {
    match raw.split_once("..") {
        Some((a, b)) => {
            let (a, b) = (parse_step(p, a)?, parse_step(p, b)?);
            if a > b {
                return Err(format!("« {raw} » : plage vide"));
            }
            Ok((a..=b).collect())
        }
        None => Ok(vec![parse_step(p, raw)?]),
    }
}

fn parse_step(p: &Project, raw: &str) -> Result<u8, String> {
    let last = p.last_step;
    let n: u8 = raw
        .parse()
        .map_err(|_| format!("« {raw} » n'est pas un numéro d'étape"))?;
    if n > last {
        return Err(format!("l'étape {n} n'existe pas encore (0 à {last})"));
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::TEST_PROJECT;

    const LAST_STEP: u8 = TEST_PROJECT.last_step;

    /// Analyse dans le lieu où la commande a un sens : le dossier de travail pour celles du
    /// stagiaire, le corrigé pour les autres.
    fn parse_line(line: &str) -> Result<Command, String> {
        let args: Vec<String> = line.split_whitespace().map(String::from).collect();
        let place = match args.first() {
            Some(name) if WORKDIR_ONLY.contains(&name.as_str()) => Place::Workdir,
            _ => Place::Reference,
        };
        parse(&TEST_PROJECT, place, &args)
    }

    #[test]
    fn parse_accepts_every_command() {
        assert_eq!(parse_line(""), Ok(Command::Help));
        assert_eq!(parse_line("--help"), Ok(Command::Help));
        assert_eq!(parse_line("status"), Ok(Command::Status));
        assert_eq!(parse_line("next"), Ok(Command::Next));
        assert_eq!(parse_line("verify"), Ok(Command::Verify));
        assert_eq!(parse_line("inventory"), Ok(Command::Inventory));
        assert_eq!(
            parse_line("check-steps --keep"),
            Ok(Command::CheckSteps(CheckOptions {
                keep: true,
                ..CheckOptions::default()
            }))
        );
        assert_eq!(
            parse_line("check-steps --step 3 --quick --step 1..2 --step 2"),
            Ok(Command::CheckSteps(CheckOptions {
                quick: true,
                steps: vec![1, 2, 3],
                ..CheckOptions::default()
            }))
        );
        assert_eq!(parse_line("goto 3"), Ok(Command::Goto(3)));
        assert_eq!(parse_line("goto end"), Ok(Command::GotoEnd));
        assert_eq!(parse_line("solve 0"), Ok(Command::Solve(0)));
        assert_eq!(
            parse_line(&format!("reset {LAST_STEP}")),
            Ok(Command::Reset(LAST_STEP))
        );
        assert_eq!(
            parse_line("start"),
            Ok(Command::Start {
                dir: None,
                force: false
            })
        );
        assert_eq!(
            parse_line("start --force --dir p"),
            Ok(Command::Start {
                dir: Some("p".into()),
                force: true
            })
        );
        assert_eq!(
            parse_line("update --dir p"),
            Ok(Command::Update {
                dir: Some("p".into())
            })
        );
    }

    #[test]
    fn parse_rejects_what_it_does_not_understand() {
        for line in [
            "bidule",
            "status x",
            "goto",
            "goto x",
            &format!("goto {}", LAST_STEP + 1),
            "goto 3 --force",
            "goto end 3",
            "goto fin",
            "start --dir",
            "start --dir --force",
            "start --dir a --dir b",
            "start --frce",
            "start p",
            "update --force",
            "check-steps --force",
            "check-steps --keep --keep",
            "check-steps --step",
            "check-steps --step --quick",
            "check-steps --step 3..1",
            &format!("check-steps --step 0..{}", LAST_STEP + 1),
            "inventory x",
        ] {
            assert!(parse_line(line).is_err(), "`{line}` devrait être refusé");
        }
    }

    #[test]
    fn management_commands_are_refused_in_a_workdir() {
        let parse_in = |line: &str| {
            let args: Vec<String> = line.split_whitespace().map(String::from).collect();
            parse(&TEST_PROJECT, Place::Workdir, &args)
        };
        for name in REFERENCE_ONLY {
            let error = parse_in(name).unwrap_err();
            assert!(error.contains("dépôt du corrigé"), "{error}");
        }
        assert_eq!(parse_in("goto end"), Ok(Command::GotoEnd));
        let parse_ref = |line: &str| {
            let args: Vec<String> = line.split_whitespace().map(String::from).collect();
            parse(&TEST_PROJECT, Place::Reference, &args)
        };
        for line in ["goto 0", "goto end", "next", "solve 1", "reset 1"] {
            let error = parse_ref(line).unwrap_err();
            assert!(error.contains("dossier de travail"), "{line} : {error}");
        }
        assert_eq!(parse_ref("status"), Ok(Command::Status));
        let help = usage(&TEST_PROJECT, Place::Workdir);
        assert!(
            help.contains("goto end") && !help.contains("check-steps"),
            "{help}"
        );
        let help = usage(&TEST_PROJECT, Place::Reference);
        for name in REFERENCE_ONLY {
            assert!(help.contains(&format!("cargo xtrain {name}")), "{help}");
        }
    }
}
