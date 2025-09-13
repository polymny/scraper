#!/usr/bin/env bash

# Lance une serie de generation d'exemples.
# run <offset> <limit> <id>
run() {
    # Pour afficher le progress, nombre d'espèce à traiter
    count=0
    total=$2
    thread_id=$3

    # Pour chaque espèce
    for id in $(psql -qtAX $POSTGRES_URI -c "SELECT id FROM speciess WHERE example_media_path iS NULL ORDER BY id OFFSET $1 LIMIT $2;"); do

        echo "[$(printf "%02d" $thread_id)] $count" / "$total"

        # On va d'abord chercher un média quelconque, si il existe on le trouvera vite, ça évitera de faire deux requêtes
        # lentes si jamais il n'y a pas de médias.

        # Chercher un média pour l'espece d'id id
        media=$(psql -qtAX $POSTGRES_URI -c "SELECT medias.path FROM speciess, occurrences, medias WHERE speciess.id = occurrences.species AND occurrences.id = medias.occurrence AND 200 <= medias.status_code AND medias.status_code < 400 AND medias.path IS NOT NULL AND speciess.id = $id LIMIT 1;")

        if [ -n "$media" ]; then
            # On a trouvé un média, mais peut-être il en existe un croppé, on va le chercher
            media_cropped=$(psql -qtAX $POSTGRES_URI -c "SELECT medias.path FROM speciess, occurrences, medias WHERE speciess.id = occurrences.species AND occurrences.id = medias.occurrence AND 200 <= medias.status_code AND medias.status_code < 400 AND medias.path IS NOT NULL AND medias.x IS NOT NULL AND speciess.id = $id LIMIT 1;")

            if [ -n "$media_cropped" ]; then
                media=$media_cropped
            fi

            # Il faut maintenant enregistrer le média dans la db
            psql -qtAX $POSTGRES_URI -c "UPDATE speciess SET example_media_path = '$media' WHERE speciess.id = $id;"
        fi

        count=$(($count + 1))
    done
}

# Lance tous les exemples en paralleles.
# main <threads>
main() {
    current_offset=0
    threads=$1
    total=$(psql -qtAX $POSTGRES_URI -c "SELECT count(id) FROM speciess WHERE example_media_path IS NULL;")
    limit=$(($total / $threads + 1))

    for id in `seq 1 $threads`; do
        run $current_offset $limit $id &
        current_offset=$(($current_offset + $limit))
    done

    for id in `seq 1 $threads`; do
        wait
    done
}

if [ $# -ne 1 ]; then
    main $(nproc --all)
else
    main $1
fi
