//! Vérification du dispositif d'étapes (`cargo xtrain check-steps`).
//!
//! Lancé depuis le corrigé, engendre un dossier de travail sous
//! `target/check-steps/travail` (le `travail/` du dépôt n'est jamais touché) et y rejoue le
//! parcours d'un stagiaire. Trois modes :
//!
//! **Complet** (par défaut, en CI) :
//!
//! 1. le dossier livré : rien hors de [`Project::starter_entries`], aucun marqueur de
//!    corrigé, même toolchain que le dépôt ; puis le script [`StepCheck::extra`] `start` ;
//! 2. `goto` refuse l'étape qui suit la dernière, sans rien changer ;
//! 3. pour chaque étape n, de 0 à la dernière (`goto 0`, puis `next`) : le manifeste passe à
//!    `stepN`, `cargo build` réussit, `cargo test` est rouge et tous les `todo!()` atteints
//!    sont de l'étape n, `cargo run` ([`StepCheck::run`]) ne bute pas sur une étape
//!    antérieure ; `solve n` ramène `cargo test` au vert, `cargo run` ne bute plus sur une
//!    étape résolue, et **il ne reste plus aucune remarque** fmt ni clippy (voir plus bas) ;
//! 4. tout résolu : aucun avertissement de build ([`StepCheck::deny_warnings`]),
//!    `cargo fmt --check` et `cargo clippy -D warnings` stricts ; `next` refuse d'aller plus
//!    loin, `reset` puis `solve` rouvrent et referment la dernière étape ; le script `end`.
//!
//! **`--quick`** (pour un agent, avant un commit) : le contenu livré (1), puis à chaque étape
//! l'ouverture seule — `cargo test` rouge sur l'étape n seule, ce que seul un rejeu
//! détecte —, enfin `goto end` et les contrôles stricts de (4) sur le code résolu. Ni
//! `solve` par étape, ni `cargo run`, ni fmt/clippy par étape, ni script `extra`.
//!
//! **`--step <n>`** (répétable, ou `a..b`) : seulement ces étapes, chacune ouverte par
//! `goto n` (qui remplit les précédentes), avec le contrôle complet (3) — ou celui de
//! `--quick` si les deux sont donnés. Pour un agent qui vient de toucher l'étape n.
//!
//! **fmt et clippy, étape par étape.** À l'ouverture de l'étape n (`goto n`), ses remarques
//! peuvent apparaître : ce sont des indices voulus (un paramètre inutilisé désigne ce qu'il
//! reste à écrire), rien n'y est imposé et le rapport en donne le nombre. Après `solve n`,
//! il ne doit **plus en rester aucune** : ni de l'étape n, ni des étapes suivantes, dont
//! les trous sont pourtant encore là — c'est le rôle de leurs
//! `#[cfg_attr(not(feature = "stepN"), allow(…))]` de les taire jusqu'à leur ouverture.
//!
//! Les commandes du stagiaire passent par `cargo xtrain` dans le dossier de travail : c'est
//! aussi l'outil livré qui est éprouvé. Les artefacts restent sous `target/check-steps/target`
//! d'un lancement à l'autre, ce qui évite de recompiler les dépendances.
//!
//! **Lib locale.** Lancé avec une copie locale de cette crate (`cargo --config
//! 'patch."<dépôt>".xtask-training.path="…"' xtrain check-steps`), le contrôle transmet ce
//! patch au dossier de travail, sans quoi ses `cargo xtrain` y prendraient la version
//! publiée : il l'écrit dans `target/check-steps/.cargo/config.toml`, que cargo lit depuis
//! tout dossier en dessous ([`share_local_lib`]).

use crate::blocks::{SOLUTION_BEGIN, SOLUTION_END, todo_steps};
use crate::files::relative;
use crate::project::{Project, REPOSITORY, TOOL_DIR};
use crate::starter::{WORKDIR, ensure_reference, generate};
use crate::steps::current_step;
use std::collections::BTreeSet;
use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Instant;

/// Les options de `cargo xtrain check-steps`.
#[derive(Debug, Default, PartialEq)]
pub struct CheckOptions {
    /// `--keep` : garder le dossier de travail.
    pub keep: bool,
    /// `--quick` : l'ouverture de chaque étape et la fin seulement.
    pub quick: bool,
    /// `--step <n>` : ces étapes seulement (vide : toutes), croissantes et sans doublon.
    pub steps: Vec<u8>,
}

/// Vérifie le dispositif d'étapes du corrigé `root`.
pub fn check_steps(p: &Project, root: &Path, options: &CheckOptions) -> Result<(), String> {
    ensure_reference(p, root)?;
    let started = Instant::now();
    let base = root.join("target").join("check-steps");
    let checker = Checker {
        p,
        root,
        quick: options.quick,
        work: base.join(WORKDIR),
        target: base.join("target"),
        color: std::io::stdout().is_terminal(),
    };
    share_local_lib(&base, root)?;
    if checker.work.exists() {
        fs::remove_dir_all(&checker.work)
            .map_err(|e| format!("{} : {e}", checker.work.display()))?;
    }
    let result = if options.steps.is_empty() {
        checker.run()
    } else {
        checker.run_steps(&options.steps)
    };
    if options.keep || result.is_err() {
        println!(
            "Dossier de travail conservé : {}",
            relative(root, &checker.work)
        );
    } else {
        let _ = fs::remove_dir_all(&checker.work);
    }
    result?;
    let checked = match options.steps.as_slice() {
        [] => format!("étapes 0 à {}", p.last_step),
        [n] => format!("étape {n}"),
        steps => format!(
            "étapes {}",
            steps
                .iter()
                .map(u8::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };
    let mode = if options.quick { " (--quick)" } else { "" };
    println!(
        "\n{} en {:.0?}",
        checker.paint("32", &format!("==> {checked} vérifiée(s){mode}")),
        started.elapsed()
    );
    Ok(())
}

/// Le dossier des sources de cette crate, tel que compilé.
const LIB_DIR: &str = env!("CARGO_MANIFEST_DIR");

/// `dir` est-il une copie locale de la crate, et non celle que cargo a téléchargée (dépôt
/// git ou registre, rangés sous `$CARGO_HOME`) ?
fn is_local(dir: &str) -> bool {
    let dir = dir.replace('\\', "/");
    !["/git/checkouts/", "/registry/src/"]
        .iter()
        .any(|cached| dir.contains(cached))
}

/// Transmet au dossier de travail, sous `base`, la copie locale de la crate qui exécute
/// ce contrôle, s'il y en a une ; sinon, efface une transmission d'un lancement précédent.
///
/// Cargo lit les `.cargo/config.toml` de tous les dossiers parents du répertoire courant :
/// celui de `base` vaut pour toute commande lancée dans le dossier de travail, `cargo
/// xtrain` imbriqué et script [`StepCheck::extra`](crate::StepCheck::extra) compris. Il
/// reste sous `target/` : ni livré, ni lu depuis le corrigé.
fn share_local_lib(base: &Path, root: &Path) -> Result<(), String> {
    let config = base.join(".cargo").join("config.toml");
    if !is_local(LIB_DIR) {
        if config.exists() {
            fs::remove_file(&config).map_err(|e| format!("{} : {e}", config.display()))?;
        }
        return Ok(());
    }
    let text = format!(
        "# Écrit par `cargo xtrain check-steps` : la copie locale de xtask-training qui\n\
         # exécute le contrôle, transmise au dossier de travail.\n\
         [patch.\"{REPOSITORY}\"]\n\
         xtask-training = {{ path = '{LIB_DIR}' }}\n"
    );
    let dir = config.parent().expect("config.toml a un dossier parent");
    fs::create_dir_all(dir).map_err(|e| format!("{} : {e}", dir.display()))?;
    fs::write(&config, text).map_err(|e| format!("{} : {e}", config.display()))?;
    println!(
        "==> lib locale : {LIB_DIR}\n   transmise au dossier de travail ({})",
        relative(root, &config)
    );
    Ok(())
}

struct Checker<'a> {
    p: &'a Project,
    root: &'a Path,
    quick: bool,
    work: PathBuf,
    target: PathBuf,
    color: bool,
}

/// Où `cargo run` s'est arrêté.
#[derive(Debug, PartialEq)]
enum Reached {
    Success,
    Todo(u8),
}

/// Les remarques de fmt et de clippy à un instant donné, sans numéro de ligne.
#[derive(Default)]
struct Remarks {
    fmt: BTreeSet<String>,
    clippy: BTreeSet<String>,
}

impl Checker<'_> {
    /// Le mode complet, ou `--quick` : toutes les étapes, puis la fin.
    fn run(&self) -> Result<(), String> {
        let last = self.p.last_step;
        println!("==> étapes 0 à {last}");
        self.start()?;
        self.refuses_beyond_the_last_step()?;
        for n in 0..=last {
            let open = if n == 0 { Open::Goto } else { Open::Next };
            self.step(n, open)?;
        }
        if self.quick {
            println!("==> goto end");
            self.succeed(self.xtrain(&["goto", "end"])?, "goto end")?;
        }
        self.resolved()?;
        if !self.quick {
            self.next_refused_and_reopen()?;
            self.extra("end")?;
        }
        Ok(())
    }

    /// `--step` : ces étapes seulement, chacune ouverte par `goto n`.
    fn run_steps(&self, steps: &[u8]) -> Result<(), String> {
        self.start()?;
        for &n in steps {
            self.step(n, Open::Goto)?;
        }
        Ok(())
    }

    /// Engendre le dossier de travail et en vérifie le contenu.
    fn start(&self) -> Result<(), String> {
        println!("==> start --dir {}", relative(self.root, &self.work));
        fs::create_dir_all(&self.work).map_err(|e| e.to_string())?;
        let holes = generate(self.p, self.root, &self.work)?;
        self.delivered()?;
        self.ok(&format!(
            "dossier de travail engendré ({holes} trous), sans corrigé"
        ));
        if !self.quick {
            self.extra("start")?;
        }
        Ok(())
    }

    fn refuses_beyond_the_last_step(&self) -> Result<(), String> {
        let (p, last) = (self.p, self.p.last_step);
        let sentinel = last + 1;
        println!("==> goto refuse l'étape {sentinel}, qui n'existe pas encore");
        if self
            .xtrain(&["goto", &sentinel.to_string()])?
            .status
            .success()
        {
            return Err(format!(
                "goto {sentinel} réussit : last_step ({last}) et {} divergent",
                p.step_manifest
            ));
        }
        self.expect_step(0, &format!("goto {sentinel} a changé l'étape courante"))?;
        self.ok(&format!("goto {sentinel} refusé, toujours à l'étape 0"));
        Ok(())
    }

    /// Tout est résolu : plus aucun avertissement, ni de build, ni de fmt, ni de clippy.
    fn resolved(&self) -> Result<(), String> {
        let check = &self.p.step_check;
        let last = self.p.last_step;
        println!("==> tout résolu : vert, sans avertissement");
        self.expect_step(last, "le dossier de travail n'est pas à la dernière étape")?;
        self.succeed(
            self.cargo("test", check.test_args)?,
            "cargo test, tout résolu",
        )?;
        self.ok("cargo test vert");
        if check.deny_warnings {
            let build = self.succeed(self.cargo("build", check.build_args)?, "cargo build")?;
            let warnings = build.lines().filter(|l| l.starts_with("warning")).count();
            if warnings > 0 {
                return Err(format!(
                    "{warnings} avertissement(s) une fois toutes les étapes résolues\n{build}"
                ));
            }
            self.ok("aucun avertissement de build");
        }
        let remarks = self.remarks()?;
        if !remarks.fmt.is_empty() || !remarks.clippy.is_empty() {
            return Err(format!(
                "tout résolu, il reste des remarques :\n{}",
                remarks.describe()
            ));
        }
        if check.fmt.is_some() || check.clippy.is_some() {
            self.ok("fmt et clippy propres");
        }
        Ok(())
    }

    fn next_refused_and_reopen(&self) -> Result<(), String> {
        let check = &self.p.step_check;
        let last = self.p.last_step;
        println!("==> next refuse d'aller au-delà de l'étape {last}");
        if self.xtrain(&["next"])?.status.success() {
            return Err("next réussit à la dernière étape".into());
        }
        self.expect_step(last, "next a changé l'étape courante alors qu'il a échoué")?;
        self.ok(&format!("next refusé, toujours à l'étape {last}"));

        println!("==> reset {last} rouvre les trous, solve {last} les referme");
        self.succeed(self.xtrain(&["reset", &last.to_string()])?, "reset")?;
        if self.cargo("test", check.test_args)?.status.success() {
            return Err(format!(
                "cargo test vert alors que l'étape {last} vient d'être rouverte"
            ));
        }
        self.ok("rouge une fois l'étape rouverte");
        self.succeed(self.xtrain(&["solve", &last.to_string()])?, "solve")?;
        self.succeed(
            self.cargo("test", check.test_args)?,
            "cargo test après solve",
        )?;
        self.ok("vert de nouveau");
        Ok(())
    }

    /// L'étape `n` : l'ouvrir, constater le rouge ; hors `--quick`, la résoudre et
    /// constater le vert, sans remarque nouvelle.
    fn step(&self, n: u8, open: Open) -> Result<(), String> {
        self.open(n, open)
            .and_then(|before| {
                if self.quick {
                    Ok(())
                } else {
                    self.solve(n, before)
                }
            })
            .map_err(|e| format!("étape {n} : {e}"))
    }

    /// Ouvre l'étape `n` et vérifie le rouge ; renvoie les remarques fmt/clippy relevées à
    /// l'ouverture (vides en `--quick`).
    fn open(&self, n: u8, open: Open) -> Result<Remarks, String> {
        let check = &self.p.step_check;
        println!("==> étape {n}");
        let (verb, args) = match open {
            Open::Goto => (format!("goto {n}"), vec!["goto".to_string(), n.to_string()]),
            Open::Next => ("next".to_string(), vec!["next".to_string()]),
        };
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        println!("    {verb}");
        self.succeed(self.xtrain(&args)?, &verb)?;
        self.expect_step(
            n,
            &format!("{verb} n'a pas fait passer le manifeste à step{n}"),
        )?;

        println!("    build");
        self.succeed(
            self.cargo("build", check.build_args)?,
            "cargo build avant solve",
        )?;

        println!("    test (rouge attendu, sur l'étape {n} seulement)");
        let output = self.cargo("test", check.test_args)?;
        let text = text(&output);
        if output.status.success() {
            return Err("cargo test déjà vert alors que l'étape vient d'être ouverte".into());
        }
        let reached: BTreeSet<u8> = todo_steps(&text).into_iter().collect();
        if reached.is_empty() {
            return Err(format!(
                "cargo test rouge sans todo!() reconnaissable\n{}",
                tail(&text)
            ));
        }
        if reached != BTreeSet::from([n]) {
            return Err(format!(
                "todo!() atteints sur les étapes {reached:?}, au lieu de {n} seule"
            ));
        }
        self.ok(&format!("rouge sur l'étape {n}"));

        if self.quick {
            return Ok(Remarks::default());
        }
        if check.run.is_some() {
            println!("    run (todo!() attendu sur l'étape {n} ou une suivante, jamais avant)");
            match self.run_program()? {
                Reached::Todo(s) if s < n => {
                    return Err(format!("cargo run bute sur l'étape {s}, antérieure"));
                }
                Reached::Todo(s) => self.ok(&format!("todo!() atteint sur l'étape {s} (≥ {n})")),
                Reached::Success => self.ok("cargo run réussit"),
            }
        }
        let remarks = self.remarks()?;
        if check.fmt.is_some() || check.clippy.is_some() {
            println!(
                "    fmt, clippy : {} remarque(s) à l'ouverture (indices, rien d'imposé)",
                remarks.len()
            );
        }
        Ok(remarks)
    }

    /// Résout l'étape `n` : vert, `cargo run` qui ne bute plus sur elle, et **plus aucune**
    /// remarque fmt/clippy — ni de l'étape n, ni des suivantes, que leurs `cfg_attr`
    /// doivent taire jusqu'à leur ouverture. `before` (les remarques à l'ouverture, ses
    /// indices) ne sert qu'au compte rendu.
    fn solve(&self, n: u8, before: Remarks) -> Result<(), String> {
        let check = &self.p.step_check;
        println!("    solve {n}");
        self.succeed(self.xtrain(&["solve", &n.to_string()])?, "solve")?;
        println!("    test (doit être entièrement vert)");
        self.succeed(
            self.cargo("test", check.test_args)?,
            "cargo test après solve",
        )?;
        self.ok("cargo test vert");

        if check.fmt.is_some() || check.clippy.is_some() {
            let after = self.remarks()?;
            if after.len() > 0 {
                return Err(format!(
                    "après solve {n}, il reste {} remarque(s) — de l'étape {n} si sa \
                     solution les laisse, d'une étape suivante s'il lui manque un \
                     `cfg_attr(not(feature = \"stepN\"), allow(…))` :\n{}",
                    after.len(),
                    after.describe()
                ));
            }
            self.ok(&format!(
                "propre après solve ({} remarque(s) à l'ouverture)",
                before.len()
            ));
        }

        if check.run.is_some() {
            println!("    run (todo!() attendu sur une étape > {n}, ou succès)");
            match self.run_program()? {
                Reached::Todo(s) if s <= n => {
                    return Err(format!(
                        "après solve, cargo run bute encore sur l'étape {s}"
                    ));
                }
                Reached::Todo(s) => self.ok(&format!("todo!() atteint sur l'étape {s} (> {n})")),
                Reached::Success => self.ok("cargo run réussit"),
            }
        }
        Ok(())
    }

    /// Les remarques de `cargo fmt --check` et de `cargo clippy`, selon [`StepCheck`].
    ///
    /// Une remarque est identifiée par son fichier et son message, sans ligne ni colonne :
    /// la solution d'une étape décale les lignes qui la suivent.
    fn remarks(&self) -> Result<Remarks, String> {
        let check = &self.p.step_check;
        let mut remarks = Remarks::default();
        if let Some(args) = check.fmt {
            let mut args = args.to_vec();
            args.push("--check");
            let output = self.cargo("fmt", &args)?;
            remarks.fmt = fmt_remarks(&text(&output), &self.work);
            if !output.status.success() && remarks.fmt.is_empty() {
                return Err(format!(
                    "cargo fmt --check échoue\n{}",
                    tail(&text(&output))
                ));
            }
        }
        if let Some(args) = check.clippy {
            let mut args = args.to_vec();
            args.push("--message-format=short");
            let output = self.cargo("clippy", &args)?;
            if !output.status.success() {
                return Err(format!("cargo clippy échoue\n{}", tail(&text(&output))));
            }
            remarks.clippy = clippy_remarks(&text(&output));
        }
        Ok(remarks)
    }

    /// Le dossier livré ne contient que ce qu'il doit, sans marqueur de corrigé.
    fn delivered(&self) -> Result<(), String> {
        let p = self.p;
        let allowed: BTreeSet<&str> = p
            .starter_entries
            .iter()
            .map(|e| e.split('/').next().unwrap_or(e))
            .chain([TOOL_DIR])
            .collect();
        let entries = fs::read_dir(&self.work).map_err(|e| e.to_string())?;
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !allowed.contains(name.as_str()) {
                return Err(format!(
                    "le dossier de travail contient {name}, qui n'est pas à livrer"
                ));
            }
        }
        for path in p.corrige_only_paths {
            if self.work.join(path).exists() {
                return Err(format!(
                    "le dossier de travail contient {path}, réservé au corrigé"
                ));
            }
        }

        let toolchain = "rust-toolchain.toml";
        if self.root.join(toolchain).is_file()
            && fs::read(self.root.join(toolchain)).ok() != fs::read(self.work.join(toolchain)).ok()
        {
            return Err(format!(
                "le dossier de travail n'a pas le même {toolchain} que le dépôt"
            ));
        }

        let mut files = Vec::new();
        walk(&self.work, &mut files);
        for path in files {
            let key = relative(&self.work, &path);
            if key.starts_with(&format!("{TOOL_DIR}/")) {
                continue;
            }
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            let leak = |line: &str| {
                let line = line.trim();
                match path.extension().and_then(|e| e.to_str()) {
                    Some("rs") => {
                        line.starts_with(SOLUTION_BEGIN)
                            || line.starts_with("// SOLUTION-ELSE")
                            || line.starts_with(SOLUTION_END)
                            || line.starts_with("// CORRIGE-ONLY-FILE")
                    }
                    Some("md") => line.starts_with("<!-- CORRIGE-ONLY-"),
                    _ => false,
                }
            };
            if let Some(line) = text.lines().position(leak) {
                return Err(format!(
                    "{key}:{} : marqueur de corrigé livré au stagiaire",
                    line + 1
                ));
            }
        }
        Ok(())
    }

    /// Lance le script [`StepCheck::extra`], s'il y en a un.
    fn extra(&self, phase: &str) -> Result<(), String> {
        let Some(script) = self.p.step_check.extra else {
            return Ok(());
        };
        println!("==> {script} {phase}");
        let status = Command::new("bash")
            .arg(self.root.join(script))
            .arg(phase)
            .current_dir(&self.work)
            .env("CARGO_TARGET_DIR", &self.target)
            .env("XTRAIN_ROOT", self.root)
            .env("XTRAIN_WORKDIR", &self.work)
            .env("XTRAIN_LAST_STEP", self.p.last_step.to_string())
            .status()
            .map_err(|e| format!("{script} : {e}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("{script} {phase} : échec"))
        }
    }

    /// `cargo run` ([`StepCheck::run`]) : succès, ou l'étape du premier `todo!()` atteint.
    fn run_program(&self) -> Result<Reached, String> {
        let args = self.p.step_check.run.unwrap_or(&[]);
        let output = self.cargo("run", args)?;
        if output.status.success() {
            return Ok(Reached::Success);
        }
        let text = text(&output);
        todo_steps(&text)
            .first()
            .map(|&s| Reached::Todo(s))
            .ok_or_else(|| {
                format!(
                    "cargo run échoue sans todo!() reconnaissable\n{}",
                    tail(&text)
                )
            })
    }

    fn expect_step(&self, step: u8, error: &str) -> Result<(), String> {
        match current_step(self.p, &self.work) {
            Some(s) if s == step => Ok(()),
            other => Err(format!(
                "{error} (étape lue : {other:?}, attendue : {step})"
            )),
        }
    }

    /// `cargo <command> <args>` dans le dossier de travail, sortie capturée.
    fn cargo(&self, command: &str, args: &[&str]) -> Result<Output, String> {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        Command::new(&cargo)
            .arg(command)
            .args(args)
            .current_dir(&self.work)
            .env("CARGO_TARGET_DIR", &self.target)
            .output()
            .map_err(|e| format!("{cargo} {command} : {e}"))
    }

    fn xtrain(&self, args: &[&str]) -> Result<Output, String> {
        self.cargo("xtrain", args)
    }

    /// La sortie d'une commande qui doit réussir, ou une erreur qui en montre la fin.
    fn succeed(&self, output: Output, what: &str) -> Result<String, String> {
        let text = text(&output);
        if output.status.success() {
            Ok(text)
        } else {
            Err(format!("{what} échoue\n{}", tail(&text)))
        }
    }

    fn ok(&self, message: &str) {
        println!("   {} {message}", self.paint("32", "ok"));
    }

    fn paint(&self, color: &str, text: &str) -> String {
        if self.color {
            format!("\x1b[{color}m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }
}

/// Comment ouvrir une étape.
#[derive(Clone, Copy)]
enum Open {
    Goto,
    Next,
}

impl Remarks {
    fn len(&self) -> usize {
        self.fmt.len() + self.clippy.len()
    }

    fn describe(&self) -> String {
        let fmt = self
            .fmt
            .iter()
            .map(|f| format!("  fmt : {f} n'est pas formaté"));
        let clippy = self.clippy.iter().map(|c| format!("  clippy : {c}"));
        fmt.chain(clippy).collect::<Vec<_>>().join("\n")
    }
}

/// Les fichiers que `cargo fmt --check` voudrait reformater (`Diff in <fichier>:<ligne>:`,
/// ou `Diff in <fichier> at line <n>:` selon la version), relatifs à `work`.
fn fmt_remarks(output: &str, work: &Path) -> BTreeSet<String> {
    output
        .lines()
        .filter_map(|line| line.strip_prefix("Diff in "))
        .filter_map(|rest| {
            let end = rest.find(".rs")? + 3;
            Some(relative(work, Path::new(&rest[..end])))
        })
        .collect()
}

/// Les remarques de `cargo clippy --message-format=short`
/// (`<fichier>:<ligne>:<colonne>: warning: <message>`), sans ligne ni colonne.
fn clippy_remarks(output: &str) -> BTreeSet<String> {
    output
        .lines()
        .filter_map(|line| {
            let (file, rest) = line.split_once(':')?;
            let (row, rest) = rest.split_once(':')?;
            let (column, message) = rest.split_once(": ")?;
            let numeric = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
            (numeric(row) && numeric(column) && !file.contains(' ')).then(|| {
                let message = message.split(": help:").next().unwrap_or(message);
                format!("{file} : {message}")
            })
        })
        .collect()
}

/// Sorties standard et d'erreur d'une commande, à la suite.
fn text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// Les dernières lignes d'une sortie, pour un message d'erreur.
fn tail(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(30)..].join("\n")
}

/// Tous les fichiers sous `dir`, `target/` exclus.
fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if entry.file_name() != "target" {
                walk(&path, out);
            }
        } else {
            out.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clippy_remarks_drop_positions_and_help() {
        let output = "\
    Checking wind-tunnel v0.1.0
src/mesh.rs:596:12: warning: writing `&mut Vec` instead of `&mut [_]`: help: change this to: `&mut [Face]`
src/solver.rs:563:41: warning: unused variable: `k1`: help: if this is intentional, prefix it with an underscore: `_k1`
src/solver.rs:579:41: warning: unused variable: `k1`: help: if this is intentional, prefix it with an underscore: `_k1`
warning: `wind-tunnel` (lib) generated 3 warnings
";
        let remarks: Vec<String> = clippy_remarks(output).into_iter().collect();
        assert_eq!(
            remarks,
            [
                "src/mesh.rs : warning: writing `&mut Vec` instead of `&mut [_]`",
                "src/solver.rs : warning: unused variable: `k1`",
            ]
        );
    }

    #[test]
    fn a_downloaded_crate_is_not_local() {
        assert!(!is_local(
            "/home/u/.cargo/git/checkouts/xtask-training-1a2b/5a3bfbf"
        ));
        assert!(!is_local(
            "/home/u/.cargo/registry/src/index.crates.io-1a2b/xtask-training-0.1.2"
        ));
        assert!(!is_local(
            r"C:\Users\u\.cargo\git\checkouts\xtask-training-1a2b\5a3bfbf"
        ));
        assert!(is_local("/home/u/exercises/xtask-training"));
    }

    #[test]
    fn fmt_remarks_name_the_files() {
        let work = Path::new("/w");
        let output = "Diff in /w/src/a.rs:12:\n-x\n+y\nDiff in /w/src/b.rs at line 3:\n";
        let remarks: Vec<String> = fmt_remarks(output, work).into_iter().collect();
        assert_eq!(remarks, ["src/a.rs", "src/b.rs"]);
    }
}
