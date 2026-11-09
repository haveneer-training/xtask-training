//! Ce qui distingue un fil rouge d'un autre : l'outillage lui-même est commun.

use crate::files::relative;
use std::fs;

/// Répertoire de la crate `xtrain` dans le dépôt, et du dossier de travail où sont
/// rangés les blocs de référence et les énoncés livrés.
pub const TOOL_DIR: &str = "xtrain";

/// La description d'un fil rouge, tous chemins relatifs à la racine de son dépôt.
///
/// Écrite une fois pour toutes dans la crate `xtrain/` du dépôt ; [`check`] vérifie
/// qu'elle reste d'accord avec lui.
#[derive(Debug)]
pub struct Project {
    /// Nom du fil rouge, tel que l'aide l'affiche : « Outillage du fil rouge « {name} » ».
    pub name: &'static str,

    /// Dernière étape couverte par le code actuel. Le manifeste des étapes déclare une
    /// feature de plus, `step{last_step + 1}`, la **sentinelle** : la ligne `default` du
    /// corrigé la porte, `goto` la refuse.
    pub last_step: u8,

    /// Manifeste qui porte les features `stepN` chaînées ; `goto` réécrit sa ligne
    /// `default = [...]`.
    pub step_manifest: &'static str,

    /// Autres manifestes dont la ligne `default = [...]` porte la sentinelle : le dossier
    /// de travail la vide. `goto` n'y touche pas (une crate qui suit les features de la
    /// bibliothèque, par exemple).
    pub sentinel_manifests: &'static [&'static str],

    /// Répertoires de sources fouillés par le dispositif d'étapes : ceux dont les blocs
    /// `SOLUTION-BEGIN` deviennent des trous. Les répertoires `tests/` n'y sont jamais :
    /// leurs fichiers sont copiés tels quels, ce qui garantit qu'aucun test ne se fait
    /// trouer par mégarde.
    pub source_dirs: &'static [&'static str],

    /// Ce que le dossier de travail reprend du corrigé : fichiers et arborescences.
    ///
    /// Doit comprendre `.cargo` (l'alias `xtrain`), `xtrain/Cargo.toml` et `xtrain/src` ;
    /// jamais `solutions/`, ni `target/`, ni le dossier de travail lui-même.
    pub starter_entries: &'static [&'static str],

    /// Arborescences hors fil rouge dont le code porte des blocs de corrigé (mêmes blocs
    /// que les exercices courts) : le dossier de travail reçoit l'énoncé de chaque fichier.
    /// Elles ne servent pas aux étapes (`goto`, `solve`) : leurs corrigés restent à
    /// consulter dans le dépôt.
    pub corrige_trees: &'static [&'static str],

    /// Ce que le dossier de travail ne reprend pas, bien que sous une entrée de
    /// [`Project::starter_entries`] : l'outillage du formateur, par exemple.
    pub corrige_only_paths: &'static [&'static str],

    /// Modules dotés d'exercices courts, dans `exercices/<m>/examples`.
    pub short_exercises: &'static [&'static str],

    /// Les exercices courts du module `m` sont les exemples de la crate
    /// `{exercise_package_prefix}-m`.
    pub exercise_package_prefix: &'static str,

    /// Crates du dépôt qu'un exercice court peut importer, `(nom du paquet, répertoire)` :
    /// la crate jetable où `verify` compile un tel énoncé en dépend.
    pub exercise_dependencies: &'static [(&'static str, &'static str)],

    /// Ajouté tel quel à la fin du manifeste de cette crate jetable (un profil, pour
    /// réutiliser les artefacts du workspace) ; vide le plus souvent.
    pub exercise_manifest_extra: &'static str,

    /// Ce que `cargo xtrain check-steps` vérifie à chaque étape.
    pub step_check: StepCheck,
}

/// Comment `cargo xtrain check-steps` éprouve le dispositif d'étapes (voir
/// `src/check.rs`). Partir de [`StepCheck::DEFAULT`] et n'en changer que l'écart :
/// `StepCheck { extra: Some("scripts/check-steps-extra.sh"), ..StepCheck::DEFAULT }`.
#[derive(Debug)]
pub struct StepCheck {
    /// Arguments de `cargo build` à chaque étape, et pour compter les avertissements.
    pub build_args: &'static [&'static str],

    /// Arguments de `cargo test` : rouge à l'ouverture de l'étape n, tous les `todo!()`
    /// atteints étant ceux de l'étape n (un test d'étape ancienne peut redevenir rouge,
    /// mais seulement sur un trou de l'étape ouverte) ; vert après `solve n`.
    pub test_args: &'static [&'static str],

    /// Arguments de `cargo run`, lancé avant et après `solve` de chaque étape : il réussit,
    /// ou bute sur le `todo!()` d'une étape pas encore résolue — jamais d'une étape
    /// antérieure. À la dernière étape, il doit réussir. `None` : pas de `cargo run`.
    pub run: Option<&'static [&'static str]>,

    /// Aucun avertissement de `cargo build` une fois toutes les étapes résolues.
    pub deny_warnings: bool,

    /// Arguments de `cargo clippy`. À l'ouverture d'une étape, ses remarques sont permises
    /// (des indices) ; après `solve n`, plus aucune ne doit rester, ni de l'étape n, ni des
    /// suivantes. `None` : pas de clippy.
    pub clippy: Option<&'static [&'static str]>,

    /// Arguments de `cargo fmt --check`, avec la même règle : tout est formaté après
    /// chaque `solve`. `None` : pas de fmt.
    pub fmt: Option<&'static [&'static str]>,

    /// Script du dépôt (`bash <script> start|end`) pour les contrôles propres au fil
    /// rouge, lancé dans le dossier de travail après `start` puis après la dernière étape.
    /// Il reçoit `XTRAIN_ROOT`, `XTRAIN_WORKDIR` et `XTRAIN_LAST_STEP`, et peut appeler
    /// `cargo xtrain`.
    pub extra: Option<&'static str>,
}

impl StepCheck {
    /// Le réglage par défaut : `cargo test` sans argument ; clippy (toutes les cibles) et
    /// fmt (tout le workspace) sans aucune remarque après chaque `solve` ; aucun
    /// avertissement de build une fois tout résolu.
    pub const DEFAULT: StepCheck = StepCheck {
        build_args: &["--all-targets"],
        test_args: &[],
        run: None,
        deny_warnings: true,
        clippy: Some(&["--all-targets"]),
        fmt: Some(&["--all"]),
        extra: None,
    };
}

impl Project {
    /// Le paquet des exercices courts du module `module`.
    pub(crate) fn exercise_package(&self, module: &str) -> String {
        format!("{}-{module}", self.exercise_package_prefix)
    }
}

/// Vérifie que la description `project` correspond au dépôt dont `manifest_dir` est la
/// crate `xtrain/` (son `env!("CARGO_MANIFEST_DIR")`).
///
/// À appeler depuis un test de cette crate : un chemin renommé ou une étape ajoutée sans
/// mettre la description à jour casserait sinon `start` ou `goto` sans bruit.
pub fn check(project: &Project, manifest_dir: &str) -> Result<(), String> {
    let root = crate::repository_root(manifest_dir);
    let mut problems = Vec::new();

    match fs::read_to_string(root.join(project.step_manifest)) {
        Ok(manifest) => {
            let highest = highest_step_feature(&manifest);
            if highest != Some(project.last_step + 1) {
                problems.push(format!(
                    "{} : la feature stepN la plus haute ({}) devrait être la sentinelle, \
                     step{} (last_step + 1)",
                    project.step_manifest,
                    highest.map_or("aucune".to_string(), |n| format!("step{n}")),
                    project.last_step + 1
                ));
            }
        }
        Err(e) => problems.push(format!("{} : {e}", project.step_manifest)),
    }

    let required = [".cargo", "xtrain/Cargo.toml", "xtrain/src"];
    for entry in required {
        if !project.starter_entries.contains(&entry) {
            problems.push(format!("starter_entries : il manque « {entry} »"));
        }
    }

    let paths = project
        .starter_entries
        .iter()
        .chain(project.source_dirs)
        .chain(project.sentinel_manifests)
        .chain(project.corrige_trees)
        .chain(project.corrige_only_paths)
        .chain(project.exercise_dependencies.iter().map(|(_, dir)| dir))
        .chain(&project.step_check.extra);
    for path in paths {
        if !root.join(path).exists() {
            problems.push(format!("{path} : introuvable dans le dépôt"));
        }
    }
    for module in project.short_exercises {
        let dir = root.join("exercices").join(module).join("examples");
        if !dir.is_dir() {
            problems.push(format!("{} : introuvable", relative(&root, &dir)));
        }
    }

    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems.join("\n"))
    }
}

/// La plus haute feature `stepN` déclarée par un manifeste.
///
/// `scripts/check-steps.sh` en déduit la dernière étape de la même façon.
fn highest_step_feature(manifest: &str) -> Option<u8> {
    manifest
        .lines()
        .filter_map(|line| line.strip_prefix("step")?.split_once(" = ")?.0.parse().ok())
        .max()
}

/// Un fil rouge fictif, pour les tests de l'outillage.
#[cfg(test)]
pub(crate) const TEST_PROJECT: Project = Project {
    name: "Essai",
    last_step: 4,
    step_manifest: "Cargo.toml",
    sentinel_manifests: &[],
    source_dirs: &["src"],
    starter_entries: &[
        ".cargo",
        "Cargo.toml",
        "src",
        "xtrain/Cargo.toml",
        "xtrain/src",
    ],
    corrige_trees: &[],
    corrige_only_paths: &[],
    short_exercises: &["m1"],
    exercise_package_prefix: "essai",
    exercise_dependencies: &[],
    exercise_manifest_extra: "",
    step_check: StepCheck::DEFAULT,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_highest_step_feature_is_read_from_the_manifest() {
        let manifest = "[features]\ndefault = [\"step3\"]\nstep0 = []\nstep1 = [\"step0\"]\n\
                        step10 = [\"step1\"]\nstep2 = [\"step1\"]\n";
        assert_eq!(highest_step_feature(manifest), Some(10));
        assert_eq!(highest_step_feature("[features]\n"), None);
    }
}
