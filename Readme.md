# Développement de l'application PMOMusic en RUST

## Création de la structure

```bash
mkdir pizzicato
cd pizzicato
jj git init
touch Readme.md
```

Maintenant on peu créer l'application PMOMusic

```bash
cargo new PMOMusic
```

On ajoute un fichier `Cargo.toml` décrivant le workspace pizzicato qui ne contient que notre nouvelle application

```
[workspace]
members = ["PMOMusic"]
```

On crée le package `pmoupnp`

```bash
cargo new  pmoupnp --lib
```

Cela modifie automatiquement le fichier `Cargo.toml` créé juste avant.
Dans ce package un sous module `statevariable`

```bash
cd pmoupnp/src
mkdir statevariable
```


# Petite expériences jujutsu

- Je veux voir l'historique

```bash
jj log
````
```
@  lkmlpmnk eric@coissac.eu 2025-09-12 09:48:38 a27b6a9a
│  On commence les states variables
○  mzvokmpk eric@coissac.eu 2025-09-12 09:32:27 926827f3
│  Retire le répertoir target du suivi
○  skknrvut eric@coissac.eu 2025-09-12 09:05:27 cbad5c34
│  Initialisation des l'arborescence de répertoires
◆  zzzzzzzz root() 00000000
```

- Je veux me mettre dans un commit:

```bash
jj edit mzvokmpk
```

je veux créer un nouveau commit à la suite d'un autre et m'y placer

```bash
jj new mzvokmpk
```

`mzvokmpk` peut être `@` pour dans le commit courant ou `@-` pour dans le parent

- je veux arreter de suivre 
  - un fichier

```bash
jj file untrack <filename> 
```

  - un répertoire

```bash
find dirname -type f -exec jj file untrack {} \;
```

Dans tous les cas ne pas oublier d'inscrire le fichier ou le repertoire dans le `.gitignore`

- je veux déplacer un commit comme un sous commit d'un aute 

```bash
jj rebase -d destination -r source
```

eventuellement faire un 

```bash
jj resolve --all
jj rebase --continue
```

pour résoudre les conflits

