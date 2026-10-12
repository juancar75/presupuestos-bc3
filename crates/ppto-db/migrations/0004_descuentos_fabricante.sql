-- SPDX-License-Identifier: GPL-3.0-or-later
-- PVP de tarifa y fabricante de cada recurso, y descuentos por fabricante.
ALTER TABLE conceptos ADD COLUMN tarifa_fabricante TEXT;
ALTER TABLE conceptos ADD COLUMN tarifa_pvp TEXT;
CREATE TABLE descuentos (
    revision_id INTEGER NOT NULL REFERENCES revisiones(id) ON DELETE CASCADE,
    fabricante  TEXT NOT NULL,
    porcentaje  TEXT NOT NULL,
    PRIMARY KEY (revision_id, fabricante)
);
