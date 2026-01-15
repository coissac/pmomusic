**Il faut suivre les instructions générales placées dans le fichier : Blackboard/Rules.md**

Dans l'application web: `pmoapp/webapp`, pour sa partie point de contrôle, L'interface utilisateur se met à jour en fonction des événements qui arrivent sur un canal SSE. Actuellement, il y a une logique de débouncing sur ce canal. La logique de débouncing n'est normalement pas nécessaire pour un flux SSE qui est contrôlé par le serveur.

- Supprimer cette logique de débouncing de l'application PMOControl.
