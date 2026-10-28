-- SPDX-License-Identifier: GPL-3.0-or-later
-- Procedencia del precio de un recurso (oferta y ruta de su documento), en JSON.
ALTER TABLE conceptos ADD COLUMN origen TEXT;
