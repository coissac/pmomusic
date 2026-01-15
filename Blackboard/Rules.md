# Règles de développement du projet PMO Musique.

L'application PMO Music est développée en collaboration avec un assistant de type LLM. soit Claude, soit ChatGPT, soit des petits LLM qui tournent en local sur Ollama.

## L'application PMO Music.

L'application PMO Musique est un système de gestion de la musique basé autour d'une architecture UPNP DLNA. A terme, elle contiendra trois composants:

- Un serveur de médias
- Un point de contrôle
- Un render de médias.

Les seuls médias considérés sont des médias audio. Les médias vidéo ne sont absolument pas pris en charge. L'objectif est de fournir une architecture HiFi, donc LossLess et si possible Bit-Perfect.

L'application est entièrement développée en Rust. Sauf la couche application web qui est développée en JavaScript/Typescript en se basant sur le framework [Vue](https://vuejs.org/).

## Règles de fonctionnement du Blackboard.

Le Blackboard ou tableau noir est un répertoire nommé `Blackboard` situé à la racine du projet.

L'ensemble des fichiers de ce répertoire sont des textes suivant le format Markdown.

L'arborescence de ce répertoire est la suivante:

```
Blackboard
├── Done
├── Report
├── Rules.md
├── ToDiscuss
├── ToThinkAbout
└── Todo
```

### La phase de réflexion.

- `ToThinkAbout`: Contient les tâches à réfléchir pour le projet.

Les documents dans le répertoire `ToThinkAbout` sont des fichiers contenant des idées et des questions à réfléchir pour le projet. Ils sont utilisés à terme pour générer les documents de tâche qui seront placés dans le répertoire `Todo`. Les fichiers de ce répertoire sont au format Markdown et sont écrits en collaboration entre l'humain et le LLM. Les deux ont le droit de modifier les fichiers.

### Les quatres répertoires de base pour le workflow de développement.
- `Todo`: Contient les tâches à faire pour le projet.
- `Report`: Contient les rapports de travail pour le projet.
- `ToDiscuss`: Contient les tâches à discuter pour le projet.
- `Done`: Contient les tâches terminées pour le projet.

Les tâches à faire sont décrites dans des fichiers présents dans le répertoire `Todo`. Leur réalisation conduit à la rédaction d'un rapport à placer dans le répertoire `Report`. Le rapport d'une tâche doit avoir le même nom de fichier que la tâche originale. Aucun autre rapport détaillé ne devra être produit à la fin de la tache dans le fil de la discussion. Juste une liste des documents créés ou midifiés sera donné.

À la suite du rapport, deux issues sont possibles. Soit la tâche est considérée comme achevée. Dans ce cas, elle est déplacée dans le répertoire `Done`. Soit la tâche est considérée comme incomplète. Dans ce cas, elle est déplacée dans le répertoire `ToDiscuss`. 

C'est l'humain qui décide quand une tâche peut être considérée comme *done* ou *to discuss*. En aucun cas, l'assistant peut décider de classifier une tâche après la rédaction du rapport.

Lors du déplacement d'une tâche dans le répertoire `ToDiscuss`, Le document de tâche est complété par les remarques suite à la réalisation de la première étape du travail. La poursuite d'une tâche mise en mode *to discuss* est laissée à la decision de l'humain. Lorsqu'un travail sur cette tâche est réalisé, Le rapport d'analyse correspondant est complété pour expliquer comment les annotations ont été prises en compte.

Lors du déplacement d'une tâche dans le répertoire `Done` une synthèse du document de tâche originale et du rapport est réalisée. C'est ce rapport final qui est placé dans `Done`.

- `Rules.md`: Contient les règles de développement du projet.

## Versioning

### Gestion des versions

- La version du projet est définie dans le fichier `PMOMusic/Cargo.toml`.
- La version du projet est synchronisée avec le fichier `version.txt`.
- La version du projet est incrémentée à push vers le dépôt git à jour du projet.

### Système de gestion des versions

- Le logiciel de gestion de versionning utilisé est [jj](https://github.com/jj-vcs/jj).
- L'URL du repository est: https://gargoton.petite-maison-orange.fr/eric/pmomusic.git

#### Workflow de gestion des versions

- Pour chaque nouveau travail sur le code, un nouveau commit `jj` est créé, par l'appel de la règle `jjnew` du makefile.:

```bash
make jjnew
```

  L'appel de la règle `jjnew` documente le commit en cours à partir de tous les changements du commit, puis appelle la commande `jj new`.
  
- Pour pousser une série de commit sur le serveur Git on utilise la règle `jjpush` du makefile.:

```bash
make jjpush
``` 

   - L'appel de la règle `jjpush` documente le commit en cours à partir de tous les changements du commit, puis appelle la commande `jj git push --change @`.
  
  
  
   - Cela provoque la création d'une nouvelle branche sur le serveur Git et d'un pull request. Il faut ensuite valider le pull request pour pouvoir récupérer l'état courant dans la branche de développement sur la machine.

- Pour récupérer les dernières modifications du dépôt Git, après avoir validé le pull request, on utilise la règle `jjfetch` du makefile.:

```bash
make jjfetch
```
