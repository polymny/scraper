# Scraping

### Usage

Ce scraper est prévu pour tourner dans docker, vous pouvez cloner le repository, et démarrer le docker-compose avec :
```sh
git clone https://github.com/polymny/scraper
cd scraper
docker-compose up -d --build
```

Cela va construire l'image docker puis démarrer le service et la base de données Postgres.

Il faut ensuite initialiser la base de données :
```sh
docker compose exec server scraper reset-db
```

Une fois la base de données initialisée, on peut lancer une session de scraping ainsi :
```sh
docker compose exec server scraper scrap family=Apidae
```
