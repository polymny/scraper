#!/usr/bin/env bash

# Create output dir
mkdir -p /data/csv

# Count all occurrnences
occurrences_count=$(psql -qtAX $POSTGRES_URI -c "SELECT count(id) FROM occurrences;")

# Generate occurrences_distrib.csv
psql -qAX --field-separator=',' --pset="footer=off" $POSTGRES_URI -c "
SELECT
    REPLACE(valid_name, ',', ' ') AS valid_name,
    species_key,
    count(occurrences.id) as occurrences_number,
    100.0 * count(occurrences.id) / $occurrences_count  AS occurrences_percentage
FROM
    speciess,
    occurrences
WHERE
    occurrences.species = speciess.id
GROUP BY
    speciess.valid_name, speciess.species_key
;" > /data/csv/occurrences_distrib.csv
