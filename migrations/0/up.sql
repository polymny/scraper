CREATE TABLE "species_metadatas" (
    "id" SERIAL PRIMARY KEY,
    "reign" VARCHAR,
    "phylum" VARCHAR,
    "class" VARCHAR,
    "order" VARCHAR,
    "family" VARCHAR,
    "genus" VARCHAR,
    "species" VARCHAR UNIQUE,
    "example_media_path" VARCHAR,
    "species_count" INT NOT NULL,
    "medias_count" INT NOT NULL,
    "medias_downloaded_count" INT NOT NULL,
    "medias_cropped_count" INT NOT NULL
);

CREATE TABLE "speciess" (
    "id" SERIAL PRIMARY KEY,
    "reign" VARCHAR NOT NULL,
    "phylum" VARCHAR NOT NULL,
    "class" VARCHAR NOT NULL,
    "order" VARCHAR NOT NULL,
    "family" VARCHAR NOT NULL,
    "genus" VARCHAR NOT NULL,
    "valid_name" VARCHAR NOT NULL UNIQUE,
    "species_key" BIGINT UNIQUE,
    "available_occurrences" BIGINT NOT NULL,
    "done" BOOL NOT NULL,
    "example_media_path" VARCHAR
);

CREATE TABLE "ignored_speciess" (
    "id" SERIAL PRIMARY KEY,
    "reign" VARCHAR NOT NULL,
    "phylum" VARCHAR NOT NULL,
    "class" VARCHAR NOT NULL,
    "order" VARCHAR NOT NULL,
    "family" VARCHAR NOT NULL,
    "genus" VARCHAR NOT NULL,
    "valid_name" VARCHAR NOT NULL UNIQUE,
    "species_key" BIGINT
);

CREATE TABLE "occurrences" (
    "id" SERIAL PRIMARY KEY,
    "key" BIGINT NOT NULL UNIQUE,
    "dataset_key" UUID NOT NULL,
    "species" INT NOT NULL REFERENCES speciess (id) ON DELETE CASCADE
);

CREATE TABLE "medias" (
    "id" SERIAL PRIMARY KEY,
    "url" VARCHAR NOT NULL UNIQUE,
    "path" VARCHAR,
    "status_code" INT,
    "to_download" BOOL NOT NULL,
    "cropped" BOOL NOT NULL,
    "x" DOUBLE PRECISION,
    "y" DOUBLE PRECISION,
    "width" DOUBLE PRECISION,
    "height" DOUBLE PRECISION,
    "confidence" DOUBLE PRECISION,
    "occurrence" INT NOT NULL REFERENCES occurrences (id) ON DELETE CASCADE
);
