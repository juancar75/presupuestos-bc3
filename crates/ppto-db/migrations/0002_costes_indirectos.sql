-- SPDX-License-Identifier: GPL-3.0-or-later
-- Costes indirectos de la obra y sus opciones de cálculo (como Presto).
ALTER TABLE revisiones ADD COLUMN costes_indirectos TEXT NOT NULL DEFAULT '0';
ALTER TABLE revisiones ADD COLUMN ci_redondear_coste_antes INTEGER NOT NULL DEFAULT 1;
ALTER TABLE revisiones ADD COLUMN ci_aplicar_sin_descomponer INTEGER NOT NULL DEFAULT 1;
