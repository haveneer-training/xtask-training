# xtask-training

Outillage commun des fils rouges de formation Haveneer (`cargo xtrain`) : à partir du
dépôt du corrigé, il engendre le dossier de travail du stagiaire, où les passages à écrire
deviennent des `todo!()`, puis pilote son avancement étape par étape.

Utilisé par :

- [`sentinelle`](https://github.com/haveneer-training/sentinelle) — analyse de journaux ;
- [`wind-tunnel`](https://github.com/haveneer-training/wind-tunnel) — soufflerie numérique.

## Les commandes

Depuis le dépôt du corrigé :

```shell
cargo xtrain start [--dir <p>] [--force]   # crée le dossier de travail (travail/ par défaut)
cargo xtrain update [--dir <p>]            # y reporte les modifications du corrigé
cargo xtrain verify                        # vérifie les exercices courts
cargo xtrain check-steps [--quick] [--step <n>] [--keep]   # rejoue les étapes (dossier jetable)
cargo xtrain inventory                     # inventaire des trous par étape (Markdown)
```

`check-steps` engendre un dossier de travail sous `target/check-steps/` (le `travail/` du
dépôt n'est jamais touché) et y rejoue le parcours : pour chaque étape, `goto`/`next`,
build, `cargo test` rouge sur les seuls `todo!()` de l'étape, `solve`, vert ; plus le
contenu livré (aucun marqueur de corrigé), le refus d'aller au-delà de la dernière étape,
et, tout résolu, ni avertissement ni remarque fmt/clippy.

fmt et clippy : `goto n` peut faire apparaître les remarques **de l'étape n** — des
indices, rien n'y est imposé —, mais après `solve n` il ne doit plus en rester **aucune**.
Les trous des étapes suivantes étant encore là, leurs avertissements se taisent jusqu'à
leur ouverture par un `#[cfg_attr(not(feature = "stepN"), allow(…))] // trou étape N`
(muet avant l'étape N, visible pendant, sans objet après). Dans le corrigé, la feature
sentinelle rend ces attributs inertes.

| Mode | Usage | Ce qui est rejoué |
|---|---|---|
| `check-steps` | CI | tout ce qui précède, plus `cargo run` et le script `extra` du projet |
| `check-steps --quick` | agent, avant un commit | contenu livré, rouge à l'ouverture de chaque étape, puis `goto end` et contrôles stricts du code résolu (sans fmt/clippy par étape) |
| `check-steps --step <n>` (ou `a..b`, répétable) | agent qui a touché l'étape n | `goto n`, puis le contrôle complet de cette étape (avec `--quick` : son ouverture seule) |

Le champ `step_check` du `Project` règle les arguments de build, test, `run`, fmt et
clippy ; un script du dépôt (`step_check.extra`, lancé avec `start` puis `end`) ajoute les
contrôles propres au fil rouge. Les commandes passent par le `cargo xtrain` du dossier de
travail, donc par la révision de `xtask-training` fixée dans `Cargo.lock`.

Depuis le dossier de travail :

```shell
cargo xtrain status        # où j'en suis
cargo xtrain goto <n>      # passer à l'étape n (remplit les trous restés vides avant)
cargo xtrain goto end      # aller au bout : tous les trous restés vides remplis
cargo xtrain next          # passer à l'étape suivante
cargo xtrain solve <n>     # remplir l'étape n (rattrapage)
cargo xtrain reset <n>     # la rouvrir pour la refaire
```

## Les marqueurs du corrigé

- **Trous du fil rouge** (répertoires `source_dirs`) : un bloc `// SOLUTION-BEGIN` /
  `// SOLUTION-END`, précédé d'un commentaire `// TODO-STEP:<n> …`, devient un `todo!()`
  entre `// >>> ÉTAPE n` et `// <<< ÉTAPE n`. `// SOLUTION-BEGIN vide` laisse un trou
  sans `todo!()` (des types à écrire de zéro). Le corps d'origine part dans
  `xtrain/reference.txt`, d'où `solve` et `goto` le reprennent.
- **Exercices courts** (`exercices/<m>/examples/`) et arborescences `corrige_trees` :
  `// SOLUTION-BEGIN` / `// SOLUTION-ELSE` / `// SOLUTION-END` ; l'énoncé garde la partie
  `ELSE`, décommentée. Un fichier qui commence par `// CORRIGE-ONLY-FILE` n'est pas livré.
- **Markdown** : un passage `<!-- CORRIGE-ONLY-BEGIN -->` / `<!-- CORRIGE-ONLY-END -->`
  n'existe que dans le corrigé.
- **Étapes** : le manifeste `step_manifest` déclare une feature `stepN` par étape,
  chaînée à la précédente, plus une **sentinelle** `step{last_step + 1}` que porte la
  ligne `default` du corrigé. `goto` réécrit cette ligne.

## Brancher un fil rouge

Le dépôt garde une crate binaire `xtrain/`, membre de son workspace et livrée dans le
dossier de travail :

```toml
# xtrain/Cargo.toml
[package]
name = "xtrain"
version = "0.1.0"
edition = "2024"
publish = false

[dependencies]
xtask-training = { git = "https://github.com/haveneer-training/xtask-training.git" }
```

```rust
// xtrain/src/main.rs
use xtask_training::Project;

const PROJECT: Project = Project {
    name: "Mon fil rouge",
    last_step: 5,
    step_manifest: "Cargo.toml",
    sentinel_manifests: &[],
    source_dirs: &["src"],
    starter_entries: &[
        ".gitignore", "Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "README.md",
        "src", "tests", "xtrain/Cargo.toml", "xtrain/src", ".cargo",
    ],
    corrige_trees: &[],
    corrige_only_paths: &[],
    short_exercises: &[],
    exercise_package_prefix: "mon-fil-rouge",
    exercise_dependencies: &[],
    exercise_manifest_extra: "",
    step_check: xtask_training::StepCheck::DEFAULT,
};

fn main() {
    xtask_training::main(&PROJECT, env!("CARGO_MANIFEST_DIR"));
}

#[test]
fn the_project_matches_the_repository() {
    xtask_training::check(&PROJECT, env!("CARGO_MANIFEST_DIR")).unwrap();
}
```

```toml
# .cargo/config.toml
[alias]
xtrain = "run --quiet --package xtrain --"
```

La documentation de `Project` (`cargo doc --open`) détaille chaque champ.

## Hors ligne

La dépendance git est fixée par le `Cargo.lock` du fil rouge, que le dossier de travail
reprend. Une fois `cargo xtrain start` lancé une première fois depuis le corrigé, cargo
garde la révision en cache (`~/.cargo/git`) : le dossier de travail ne refait aucun accès
réseau. Pour une salle sans réseau, `cargo vendor` embarque aussi cette dépendance.

Faire évoluer l'outillage : pousser ici, puis dans chaque fil rouge
`cargo update -p xtask-training`.

## Licence

CC BY-NC-SA 4.0 — Pascal Havé (<contact@haveneer.com>).
