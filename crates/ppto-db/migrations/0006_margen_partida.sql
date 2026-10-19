-- SPDX-License-Identifier: GPL-3.0-or-later
-- Margen propio de la partida (% sobre su precio); NULL = el general.
ALTER TABLE conceptos ADD COLUMN margen TEXT;
