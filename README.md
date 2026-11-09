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
```

Depuis le dossier de travail :

```shell
cargo xtrain status        # où j'en suis
cargo xtrain goto <n>      # passer à l'étape n (remplit les trous restés vides avant)
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
