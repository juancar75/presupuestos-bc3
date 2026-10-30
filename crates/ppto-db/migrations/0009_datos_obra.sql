-- SPDX-License-Identifier: GPL-3.0-or-later
-- Datos de la obra que no son del motor de cálculo: GG, BI e IVA del
-- resumen, Excel del cliente (ciego) del que salió la obra, etc. Clave/valor
-- por revisión, para añadir datos sin más migraciones.
CREATE TABLE datos_obra (
    revision_id INTEGER NOT NULL REFERENCES revisiones(id) ON DELETE CASCADE,
    clave       TEXT NOT NULL,
    valor       TEXT NOT NULL,
    PRIMARY KEY (revision_id, clave)
);
