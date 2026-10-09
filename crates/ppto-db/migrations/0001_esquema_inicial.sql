-- SPDX-License-Identifier: GPL-3.0-or-later
-- Esquema inicial. Los importes y cantidades se guardan como TEXT con la
-- representación decimal exacta (nunca REAL) para no perder céntimos.

CREATE TABLE presupuestos (
    id          INTEGER PRIMARY KEY,
    nombre      TEXT NOT NULL,
    creado_en   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

-- Cada revisión es una copia completa e independiente del presupuesto.
-- Modificar R2 nunca altera R1 (P-009).
CREATE TABLE revisiones (
    id                  INTEGER PRIMARY KEY,
    presupuesto_id      INTEGER NOT NULL REFERENCES presupuestos(id) ON DELETE CASCADE,
    etiqueta            TEXT NOT NULL,
    revision_origen_id  INTEGER REFERENCES revisiones(id),
    autor               TEXT NOT NULL,
    nota                TEXT,
    bloqueada           INTEGER NOT NULL DEFAULT 0 CHECK (bloqueada IN (0,1)),
    creado_en           TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    nombre              TEXT NOT NULL,
    raiz                TEXT NOT NULL,
    dec_dimensiones     INTEGER NOT NULL,
    dec_medicion        INTEGER NOT NULL,
    dec_rendimiento     INTEGER NOT NULL,
    dec_importe_linea   INTEGER NOT NULL,
    dec_precio          INTEGER NOT NULL,
    dec_importe         INTEGER NOT NULL,
    UNIQUE (presupuesto_id, etiqueta)
);

CREATE TABLE conceptos (
    revision_id INTEGER NOT NULL REFERENCES revisiones(id) ON DELETE CASCADE,
    codigo      TEXT NOT NULL,
    unidad      TEXT NOT NULL,
    resumen     TEXT NOT NULL,
    naturaleza  TEXT NOT NULL,
    precio      TEXT NOT NULL,
    texto       TEXT,
    PRIMARY KEY (revision_id, codigo)
);

CREATE TABLE descomposicion (
    revision_id INTEGER NOT NULL,
    padre       TEXT NOT NULL,
    orden       INTEGER NOT NULL,
    hijo        TEXT NOT NULL,
    factor      TEXT NOT NULL,
    rendimiento TEXT NOT NULL,
    PRIMARY KEY (revision_id, padre, orden),
    FOREIGN KEY (revision_id, padre) REFERENCES conceptos(revision_id, codigo) ON DELETE CASCADE,
    FOREIGN KEY (revision_id, hijo)  REFERENCES conceptos(revision_id, codigo)
);

CREATE TABLE medicion_lineas (
    revision_id INTEGER NOT NULL,
    padre       TEXT NOT NULL,
    hijo        TEXT NOT NULL,
    orden       INTEGER NOT NULL,
    tipo        TEXT NOT NULL,
    comentario  TEXT NOT NULL,
    unidades    TEXT,
    longitud    TEXT,
    anchura     TEXT,
    altura      TEXT,
    formula     TEXT,
    PRIMARY KEY (revision_id, padre, hijo, orden),
    FOREIGN KEY (revision_id, padre) REFERENCES conceptos(revision_id, codigo) ON DELETE CASCADE
);

-- Registro de operaciones (base para P-021: cambios vía MCP con aprobación).
CREATE TABLE auditoria (
    id          INTEGER PRIMARY KEY,
    revision_id INTEGER REFERENCES revisiones(id) ON DELETE SET NULL,
    momento     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    usuario     TEXT NOT NULL,
    operacion   TEXT NOT NULL,
    detalle     TEXT NOT NULL
);
