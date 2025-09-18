# Scraping

### Usage


##### Récupération des sources
```sh
git clone https://github.com/polymny/scraper && cd scraper
```

##### Activation du support pour GPU Nvidia (pour cropping)
```sh
cp docker-compose.nvidia.yml docker-compose.override.yml
```

##### Démarrage d'un scraping
```sh
# Création et démarrage de l'image docker de l'appli et de la base de données
docker-compose up -d --build

# Initialisation de la base de données
docker compose exec server scraper reset-db

# Démarrage du scraping sur la famille Apidae
docker compose exec server scraper scrap family=Apidae

# Calcul des médias d'exemple
docker compose exec server generate-examples

# Génération des fichiers CSV
docker compose exec server generate-csv

# Génération du cache pour plotly
docker compose exec server scraper regen-cache
```

Une fois tout ceci effectué, vous pouvez aller sur [localhost:8000](http://localhost:8000) pour naviguer dans la base de
données scrapée.

Quand vous avez fini d'utiliser le scraper, vous pouvez lancer la commande suivante pour éteindre le serveur la base de
données :

```sh
docker compose down
```
