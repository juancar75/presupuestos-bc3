-- SPDX-License-Identifier: GPL-3.0-or-later
-- Contradictorios y certificaciones de la obra (JSON del modelo
-- `ppto_core::seguimiento::Seguimiento`), una fila por revisión.
CREATE TABLE seguimiento (
    revision_id INTEGER PRIMARY KEY REFERENCES revisiones(id) ON DELETE CASCADE,
    datos       TEXT NOT NULL
);
