//! Ligne de commande : analyse d'argv et aiguillage vers les commandes.
//!
//! Analyse à la main, volontairement : l'outillage n'a aucune dépendance externe, et
//! `cargo xtrain start` est la toute première commande du stagiaire — elle doit marcher
//! hors ligne, sans attendre la compilation d'une crate comme `clap`, et répondre en
//! français. Ajouter une commande : une variante de [`Command`], un bras dans [`parse`],
//! un dans [`run`], une ligne dans [`usage`].

use crate::project::Project;
use crate::starter::{WORKDIR, make_starter};
use crate::steps::{fill, goto, next, reset, status};
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
    Next,
    Solve(u8),
    Reset(u8),
    Verify,
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
        Command::Next => next(p, root),
        Command::Solve(n) => fill(p, root, n, true),
        Command::Reset(n) => reset(p, root, n),
        Command::Verify => verify(p, root),
        Command::Help => Ok(()),
    }
}

/// Analyse la ligne de commande, nom de la commande compris.
///
/// Tout argument non reconnu est une erreur : un drapeau mal tapé (`--frce`) ne doit pas
/// être ignoré en silence.
pub fn parse(p: &Project, args: &[String]) -> Result<Command, String> {
    let Some((name, rest)) = args.split_first() else {
        return Ok(Command::Help);
    };
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

pub fn usage(p: &Project) -> String {
    let (name, last) = (p.name, p.last_step);
    format!(
        "Outillage du fil rouge « {name} »\n\
         \n\
           cargo xtrain status                où en est le projet, étape par étape\n\
           cargo xtrain goto <n>              passer à l'étape n (0 à {last})\n\
           cargo xtrain next                  passer à l'étape suivante\n\
           cargo xtrain solve <n>             remplir les trous de l'étape n\n\
           cargo xtrain reset <n>             rouvrir les trous de l'étape n\n\
           cargo xtrain start [--dir <p>]     créer le dossier de travail \
(par défaut : {WORKDIR}/)\n\
           cargo xtrain update [--dir <p>]    y reporter les modifications du corrigé\n\
           cargo xtrain verify                vérifier les exercices courts (corrigé)\n\
         \n\
         Les cinq premières commandes s'utilisent depuis le dossier de travail,\n\
         `start`, `update` et `verify` depuis le dépôt du corrigé.\n"
    )
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

    fn parse_line(line: &str) -> Result<Command, String> {
        let args: Vec<String> = line.split_whitespace().map(String::from).collect();
        parse(&TEST_PROJECT, &args)
    }

    #[test]
    fn parse_accepts_every_command() {
        assert_eq!(parse_line(""), Ok(Command::Help));
        assert_eq!(parse_line("--help"), Ok(Command::Help));
        assert_eq!(parse_line("status"), Ok(Command::Status));
        assert_eq!(parse_line("next"), Ok(Command::Next));
        assert_eq!(parse_line("verify"), Ok(Command::Verify));
        assert_eq!(parse_line("goto 3"), Ok(Command::Goto(3)));
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
            "start --dir",
            "start --dir --force",
            "start --dir a --dir b",
            "start --frce",
            "start p",
            "update --force",
        ] {
            assert!(parse_line(line).is_err(), "`{line}` devrait être refusé");
        }
    }
}
